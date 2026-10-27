## Typed bridge helpers

```ts
import { FiexUiClient } from '@fiex/ui-sdk'
const fiex = new FiexUiClient()
await fiex.clipboard.writeText('copied')
await fiex.navigation.open('fiex://mount/demo/')
await fiex.preview.ready()
const task = await fiex.plugin.task('task-id')
```

`fiex.plugin.invoke`, `fiex.dialog.*`, `fiex.tasks.*`, `fiex.panel.*`,
`fiex.theme.*` and `fiex.errors.*` are also typed. `preview.ready()` is only
available to preview documents; panels use `fiex.panel.resize/close`. There is
no UI-side VFS API: native plugins own VFS/provider work.

## Language

```ts
import { FiexUiClient, createI18n } from '@fiex/ui-sdk'
const client = new FiexUiClient()
const messages = createI18n({ en: { '打开 {0}': 'Open {0}' }, 'zh-TW': { '打开 {0}': '開啟 {0}' } })
const dispose = await client.i18n.bind(messages)
messages.t('打开 {0}', '中文.txt') // The filename is preserved.
```

`getLocale()` reads the initial host preference; `subscribe()` receives live
changes. The bridge sends `locale.changed` events and updates the HTML `lang`
attribute. All panel kinds expose this read-only information without a
permission grant. Subscribe to the translation runtime once at your React root
with `useSyncExternalStore(messages.subscribe, messages.getLocale)` so existing
forms and running operations survive language changes. Keep user and document
content out of translation lookups. Supported canonical locales are `zh-CN`,
`zh-TW` and `en`; missing messages fall back to the source text.
