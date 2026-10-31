# Provider eligibility: one FS/VFS-aware matching contract

The authoritative match DTO and evaluator are in
`rust/src/provider_matching.rs` (`ProviderMatchRule` and `LocationMatch`).
Core uses this exact SDK definition; feature handlers do not invent separate
rules based on local paths, URI schemes, or specific plugin brands.

A plugin author declares eligibility in `manifest.json`, not in its Rust
`Provider` methods. The SDK's `Providers` registry still registers the
actual implementation and its existing JSON-RPC dispatcher still executes it.
A match makes a provider a **candidate**, never an authority to read or write
the selected files. User grants, Operation Scope, ownership and effective
resource operations are checked independently.

## Location identity

A resource has a **view**, describing where it is presented, and possibly a
**storage**, describing its actual owner. Both have:

- `type`: `fs` or `vfs`
- `providerId`: registered Core-verified provider identity
- `mountId`: a single concrete mount/connection instance

`view.types` / `storage.types` filter FS versus VFS. `match.kinds` still
filters **file** or **directory** (or `connection` in a suitable event);
it never means FS/VFS.

Example: a Google Drive file shown in Recent has
`view.providerId=core:recent` and `storage.providerId` identifying the
authorized Drive provider and connection. An unavailable source has no
resolved `storage`, so a storage constraint does **not** match.

Core constructs these locations from its authenticated view and mount
registries. Plugins cannot claim `core:applications` or select a different
mount by returning JSON that contains a forged owner identity.

## Manifest examples

An Applications-only context menu:

```json
{
  "extensions": {
    "contextMenus": {
      "priority": 100,
      "match": {
        "view": {"providerIds": ["core:applications"]},
        "selection": "single"
      }
    }
  }
}
```

A generic file preview for native or plugin-backed resources:

```json
{
  "extensions": {
    "preview": {
      "match": {
        "view": {"types": ["fs", "vfs"]},
        "kinds": ["file"],
        "mimeTypes": ["image/*"],
        "requiredOps": ["read"]
      }
    }
  }
}
```

A local-only search provider:

```json
{
  "extensions": {
    "search": {
      "match": {
        "storage": {"providerIds": ["core:local-fs"]},
        "kinds": ["directory"]
      }
    }
  }
}
```

In a rule, list entries are OR; separately specified dimensions are AND.
Missing fields do not constrain; empty location selector arrays are invalid.
`view` and `storage` are independently optional, but their declared members
must be `types`, `providerIds`, or `mountIds`. No `domains` alias exists.
A declared `requiredOps` is evaluated for **every** selected resource.

The blank-canvas context menu is a zero-item selection, so use
`"selection":"zero"`; Core checks the location and operations of the
containing **directory**, not a fabricated selected file.

## Execution and security

The match is performed before network IPC. An unmatched plugin receives no
request. A matched provider is still subject to its manifest permissions,
current user grants, operation-level scope and Core output validators.
At execute time Core revalidates menu ownership, the session, and the live
resources. Native and plugin descriptors have the same eligibility semantics;
only their execution transport differs.

This contract is an incremental part of the unified provider refactor. Moving
all standard Provider traits to a runtime-independent crate, and removing the
remaining parallel event-dispatch internals, requires separate code migrations.
