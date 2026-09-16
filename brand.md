# Brand — Hedge Vault

_Status: deferred_

The user chose to defer brand setup. This project is currently using its existing
hand-rolled tokens in `app/src/app/globals.css` (emerald accent `#059669`, Geist
Sans/Mono, light theme only) and no dedicated brand palette. The
`frontend-design-guidelines` skill will quietly use these defaults and will not
prompt again.

To set up a real brand palette, typography, and voice at any time, run:

    /brand-design

or say: "pick brand colors"

When `brand-design` runs, it will detect this deferred state, skip the "confirm
overwrite" step, and proceed directly to the full brand setup. The resulting
palette will be applied to `app/src/app/globals.css` and this file will be
replaced with the real brand documentation.

_Deferred at: 2026-09-16_
