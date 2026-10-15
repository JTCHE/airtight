# Hygiene

## Global

- Directory shape: a directory holds either files or folders, never both.
  A crate's `src/` holds `main.rs` or `lib.rs` (the Cargo entry) and one
  folder per module. A module that fits in one file still gets its own folder
  with `mod.rs`. A module folder holds only files. When a module needs
  sub-modules of its own, make it a crate.
- File size: 150 to 300 code lines (blank and comment lines not counted).
  300 is the limit in `src/`, 400 in `tests/`. When a file reaches the limit,
  split it by purpose before you add more.
- Function size: 60 lines at most (JPL "Power of Ten", rule 4: one printed
  page per function). Split long functions into named steps.
- `cargo test -p airtight-hygiene` checks the directory shape and the size
  limits. Keep it green.
- Ratchet: a change must not grow a file or function that is already over the
  limit. Touch an over-limit file only to shrink it, or split it first.
- A split or a move changes no behavior and no test value. Do it in its own
  commit, with the full tests green before and after.
- One source of truth, in code and in UI. A value, a rule or a control that
  shows in two places lives in one module that both import — see # Code.
- Use ASD-STE100 Simplified Technical English in all writing: replies,
  comments, commits, pull requests.
- Do not keep backward compatibility. Delete the old path. Do not add
  fallbacks, shims, or migrations.
- Do not write documentation that a person can get from the code. Write a
  short comment in the code instead.
- Do not record a fact that goes stale: version numbers, file inventories,
  benchmark tables, audit results. Point to the code that holds it.

## Code

### One source of truth

Each value, shape, and rule lives in one place. Paths, protocol constants,
design tokens, and defaults get one definition that every caller imports. A
value written twice will drift.

If you find the same value in two files, that is a bug. Fix the duplication
before you fix the symptom.

The same holds for UI. A control that shows in two places is one component.
When a control looks or acts different in one place, make that place use the
shared one. Do not restyle the copy.

### Small modules

Split by purpose, not by size. A module does one thing and says so in its
name. Shared logic goes in a module both callers import — never copied, never
re-implemented.

Orchestration goes in the entry points and the indexes. One-purpose logic goes
in focused modules that they call.

### Dependencies

The only outside crate is `windows` (with its own helper crates). Airtight
owns its protocol code: crypto, plist, mDNS, ALAC, PTP. Do not add a crate
for something a few hundred lines can do. Read the `windows` documentation
and types before you decide that it cannot do a thing.

### Change discipline

- Fix the cause, not the symptom. Before you edit a function, find every
  caller. One guard in the shared function beats a guard in each caller.
- Choose the simplest code that meets the current requirement. Do not add
  configuration, indirection, or abstraction for a need that does not exist
  yet.
- Grow the system in layers. Start with the smallest version that works from
  end to end, then add on top of a product that already works. Never trade a
  working product for unfinished complexity.
- Make the decision for the long term. Do not accept a stopgap that you plan
  to replace.
- Delete more than you add when you can.
- Use names that explain themselves. `position_x`, not `px`.
- Write a comment only when the code cannot show the reason. Keep it to one
  or two lines, next to the code it explains. A comment that needs a
  paragraph to defend a workaround means the code is wrong — fix the code.
- Test protocol code against fixed vectors: published test vectors for
  crypto, bytes captured from a working sender for the wire formats.
