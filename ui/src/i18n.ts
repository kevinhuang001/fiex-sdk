/** Locale is an explicit host preference, never inferred from plugin content. */
export const locales = ['zh-CN', 'zh-TW', 'en'] as const
export type Locale = (typeof locales)[number]
export type Messages = Readonly<Record<string, string>>
export function normalizeLocale(value: unknown): Locale {
  const tag = String(value ?? '')
    .toLowerCase()
    .replaceAll('_', '-')
  if (/^zh-(tw|hk|mo|hant)(-|$)/.test(tag)) return 'zh-TW'
  return tag === 'en' || tag.startsWith('en-') ? 'en' : 'zh-CN'
}
export function createI18n(catalogs: Partial<Record<Locale, Messages>>, initial: Locale = 'zh-CN') {
  let locale = initial
  const listeners = new Set<() => void>()
  const getLocale = () => locale
  const subscribe = (listener: () => void) => {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }
  const setLocale = (value: unknown) => {
    const next = normalizeLocale(value)
    if (locale === next) return
    locale = next
    for (const listener of [...listeners]) listener()
  }
  const t = (source: string, ...values: unknown[]): string => {
    const catalog = catalogs[locale]
    const template =
      catalog && Object.hasOwn(catalog, source) && typeof catalog[source] === 'string'
        ? catalog[source]
        : source
    return template.replace(/\{(\d+)\}/g, (placeholder, index: string) =>
      Number(index) < values.length ? String(values[Number(index)]) : placeholder,
    )
  }
  return {
    getLocale,
    setLocale,
    subscribe,
    t,
    translateMessage: (value: string) => translateMessage(value, catalogs[locale] ?? {}),
  }
}

/** Translate a generated error message while leaving captured user data intact. */
export function translateMessage(value: string, catalog: Messages): string {
  if (Object.hasOwn(catalog, value)) return catalog[value]
  const escape = (part: string) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  for (const [source, target] of Object.entries(catalog)) {
    const slots = [...source.matchAll(/\{([\w]*)(?::[^}]*)?\}/g)]
    if (!slots.length) continue
    let position = 0,
      pattern = '^'
    const names: string[] = []
    for (const [index, slot] of slots.entries()) {
      pattern += escape(source.slice(position, slot.index)) + '([\\s\\S]*?)'
      names.push(slot[1] || String(index))
      position = slot.index! + slot[0].length
    }
    const match = new RegExp(pattern + escape(source.slice(position)) + '$').exec(value)
    if (!match) continue
    const values = new Map(names.map((name, index) => [name, match[index + 1]]))
    let index = 0
    return target.replace(
      /\{([\w]*)(?::[^}]*)?\}/g,
      (slot, name: string) => values.get(name || String(index++)) ?? slot,
    )
  }
  return value
}
