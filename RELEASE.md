# Fiex SDK 0.1.1

Terminal interfaces now belong to plugin-owned PluginViews. Removed the unused host-terminal Rust bridge and its request/task helpers. Plugins use custom UI RPC for session state and generic SDK TCP services for transport.

Typed Providers, scoped resource references and the optional Tokio adapter remain the supported plugin interfaces. The source ZIP includes the Rust SDK and procedural macros; the UI tarball includes JavaScript and TypeScript declarations.
