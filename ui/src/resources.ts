import type { ResourceMetadata } from './generated/ResourceMetadata.js'
import type { ResourceRef } from './generated/ResourceRef.js'
import type { Locale } from './i18n.js'

export type { ResourceMetadata } from './generated/ResourceMetadata.js'
export type { ResourceRef } from './generated/ResourceRef.js'

export type ResourceContext = { ref: ResourceRef; metadata?: ResourceMetadata }

export type ExplorerContext = {
  locale?: Locale
  directory?: ResourceContext
  targets: ResourceContext[]
  windowId?: string
  paneId?: string
  panelId?: string
  panelTitle?: string
  source?: 'toolbar' | 'contextMenu'
}

function record(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError('Expected an object')
  }
  return value as Record<string, unknown>
}

function optionalString(value: unknown, field: string): string | null | undefined {
  if (value === undefined || value === null || typeof value === 'string') return value
  throw new TypeError(`Invalid resource field: ${field}`)
}

function optionalInteger(value: unknown, field: string): number | null | undefined {
  if (value === undefined || value === null) return value
  if (typeof value === 'number' && Number.isSafeInteger(value)) return value
  throw new TypeError(`Invalid resource field: ${field}`)
}

export function parseResourceRef(value: unknown): ResourceRef {
  const ref = record(value)
  if (typeof ref.mountId !== 'string' || typeof ref.nodeId !== 'string') {
    throw new TypeError('Invalid resource reference')
  }
  return { mountId: ref.mountId, nodeId: ref.nodeId }
}

function parseResourceMetadata(value: unknown): ResourceMetadata {
  const metadata = record(value)
  if (typeof metadata.name !== 'string' || typeof metadata.kind !== 'string') {
    throw new TypeError('Invalid resource metadata')
  }
  const size = optionalInteger(metadata.size, 'size')
  if (size !== undefined && size !== null && size < 0) {
    throw new TypeError('Invalid resource field: size')
  }
  return {
    name: metadata.name,
    kind: metadata.kind,
    extension: optionalString(metadata.extension, 'extension'),
    mime: optionalString(metadata.mime, 'mime'),
    size,
    modified: optionalInteger(metadata.modified, 'modified'),
  }
}

export function parseResourceContext(value: unknown): ResourceContext {
  const context = record(value)
  const result: ResourceContext = { ref: parseResourceRef(context.ref) }
  if (context.metadata !== undefined && context.metadata !== null) {
    result.metadata = parseResourceMetadata(context.metadata)
  }
  return result
}

export function parseResourceContextResult(value: unknown): ResourceContext | null {
  return value === null ? null : parseResourceContext(value)
}

export function parseResourceContextList(value: unknown): ResourceContext[] | null {
  if (value === null) return null
  if (!Array.isArray(value)) throw new TypeError('Invalid resource list')
  return value.map(parseResourceContext)
}

export function parseResourcePickerOptions<T extends { directory?: ResourceRef }>(options: T): T {
  if (options.directory === undefined) return options
  return { ...options, directory: parseResourceRef(options.directory) }
}

function parseLocale(value: unknown): Locale {
  if (value === 'zh-CN' || value === 'zh-TW' || value === 'en') return value
  throw new TypeError('Invalid explorer locale')
}

export function parseExplorerContext(value: unknown): ExplorerContext {
  const context = record(value)
  if (!Array.isArray(context.targets)) throw new TypeError('Invalid explorer targets')
  const result: ExplorerContext = { targets: context.targets.map(parseResourceContext) }
  if (context.locale !== undefined) result.locale = parseLocale(context.locale)
  if (context.directory !== undefined) result.directory = parseResourceContext(context.directory)
  for (const field of ['windowId', 'paneId', 'panelId', 'panelTitle'] as const) {
    const parsed = optionalString(context[field], field)
    if (parsed !== undefined && parsed !== null) result[field] = parsed
  }
  if (context.source !== undefined) {
    if (context.source !== 'toolbar' && context.source !== 'contextMenu') {
      throw new TypeError('Invalid explorer source')
    }
    result.source = context.source
  }
  return result
}
