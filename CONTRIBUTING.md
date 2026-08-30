# Contributing to node.in.net

Thanks for taking the time to contribute.

## Developer Certificate of Origin (sign-off required)

This project does **not** use a CLA. Instead, every commit must carry a
`Signed-off-by` line, certifying the [Developer Certificate of Origin](DCO)
(the full text is in the `DCO` file at the repository root).

Git adds the line for you:

```sh
git commit -s -m "your message"
```

It looks like this, and the e-mail must match the commit author's:

```
Signed-off-by: Jane Doe <jane@example.com>
```

To never forget it, install a hook — once per clone. Note that
`git config format.signoff` does **not** do this; it only affects
`git format-patch`:

```sh
hooks="$(git rev-parse --absolute-git-dir)/hooks"
printf '%s\n' '#!/bin/sh' 'git interpret-trailers --in-place --if-exists doNothing --trailer "Signed-off-by: $(git config user.name) <$(git config user.email)>" "$1"' > "$hooks/prepare-commit-msg"
chmod +x "$hooks/prepare-commit-msg"
```

It reads `user.name` and `user.email` from git's config, runs for `git commit`
from any editor or GUI, and does not add a second line when you already
passed `-s`.

Run the same two lines from inside `submodules/p2p-common` and
`submodules/p2p-functions` — they are separate repositories with their own
sign-off requirement. A submodule keeps its hooks under the superproject's
`.git/modules/…`, not in a `.git` directory of its own, which is why the
snippet asks git for the path instead of writing `.git/hooks` literally.

Missing a sign-off on an existing commit? `git commit --amend -s` fixes the
last one; `git rebase --signoff <base>` fixes a whole branch. A CI check
enforces this on every pull request.

## Licensing of contributions

Unless you state otherwise, any contribution you submit is licensed under the
same terms as the project — **MIT OR Apache-2.0**, at the user's option. See
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

The name "node.in.net" and the project logo are not covered by the code
license.

## Getting the source

Submodules are required — the workspace does not resolve without them.

```sh
git clone --recurse-submodules https://github.com/node-in-net/app.git
```

[README.md](README.md) lists the toolchain and system libraries per platform.

## Where a change belongs

The workspace is layered, and a change usually belongs one layer lower than it
first appears:

- **`app-core`** — state and logic. No network, no GTK, no filesystem. It
  defines traits; everything else implements them. A change here is testable
  without a peer and without a window.
- **`app-net`** — the network side: routing inbound messages, the node runtime,
  identity, sign-in. The only place that knows a peer exists.
- **`app-headless`** — the REST and WebSocket surface every front end drives the
  core through, and what the test suite plays a session against. A feature that
  cannot be reached from here cannot be tested without a mouse.
- **`gtk-app`** — the GTK window. It renders what the core pushes and reports
  what the user did; it must not perform an operation itself.
- **`console-app`** — the headless node. Console output here is its interface,
  unlike everywhere else.
- **`android-node`, `wasm-node`** — thin FFI and WebAssembly wrappers around the
  same core. Logic added here would have to be written twice; put it in
  `app-core`. The Android UI itself is a Gradle project in `src/android-app`,
  outside the cargo workspace.

Protocol changes, capability implementations and shared widgets live in the
submodule repositories, not here.

## Before you open a pull request

- Keep changes focused and prefer reusing existing abstractions over adding new
  ones.
- No comments in Rust or Kotlin sources — they currently hold none, and a
  patch that adds one will be asked to remove it. The code carries its own
  explanation through names and structure; a passage that needs prose to be
  understood needs rewriting instead. Build files and CI workflows are outside
  this rule.
- No `println!` in library code. The console node is the exception. Print to stderr
  only for a real fault the user has to act on; routine tracing waits for a proper
  logging layer rather than a new environment variable.
- Run:

```sh
cargo fmt --check
cargo clippy --all-targets
cargo test
```

Formatting and tests must be clean. Clippy is not yet: the workspace still
carries 44 warnings from before it was split into its own repository. Do not add
new ones — once the backlog is cleared, `-D warnings` becomes part of CI.

A change to a submodule is a pull request against that submodule's repository,
plus a second one here moving the recorded commit.

## Reporting bugs

A good report includes the platform, how you built it, and concrete steps to
reproduce. Screen-capture and terminal problems are strongly platform-specific,
so name the desktop session (X11 or Wayland, which portal) and the OS version.
