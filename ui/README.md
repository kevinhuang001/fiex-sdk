## Language

```ts
import { FiexUiClient, createI18n } from '@fiex/ui-sdk'
const client = new FiexUiClient()
const messages = createI18n({ en: { '打开 {0}': 'Open {0}' }, 'zh-TW': { '打开 {0}': '開啟 {0}' } })
const dispose = await client.i18n.bind(messages)
messages.t('打开 {0}', '中文.txt') // The filename is preserved.
```

`getLocale()` reads the initial host preference; `subscribe()` receives live
changes. The bridge uses `locale.get` and `locale.changed`, and updates the HTML
`lang` attribute. All panel kinds expose this read-only information without a
permission grant. Subscribe to the translation runtime once at your React root
with `useSyncExternalStore(messages.subscribe, messages.getLocale)` so existing
forms and running operations survive language changes. Keep user and document
content out of translation lookups. Supported canonical locales are `zh-CN`,
`zh-TW` and `en`; missing messages fall back to the source text.
