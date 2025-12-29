# Third-Party Licenses

This project uses the following third-party libraries and components.

## SQLite

SQLite is in the public domain.

- Website: https://www.sqlite.org/
- License: Public Domain (https://www.sqlite.org/copyright.html)

Used via:
- `rusqlite` (Rust SDK)
- `better-sqlite3` (TypeScript SDK)

## libssh2

libssh2 is used for SSH connectivity.

- Website: https://libssh2.org/
- License: BSD-3-Clause
- Repository: https://github.com/libssh2/libssh2

```
Copyright (c) 2004-2007 Sara Golemon <sarag@libssh2.org>
Copyright (c) 2005,2006 Mikhail Gusarov <dottedmag@dottedmag.net>
Copyright (c) 2006-2007 The Written Word, Inc.
Copyright (c) 2007 Eli Fant <elifantu@mail.ru>
Copyright (c) 2009-2021 Daniel Stenberg
Copyright (C) 2008, 2009 Simon Josefsson

All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

Used via:
- `ssh2` crate (Rust SDK)
- `ssh2` package (TypeScript SDK)

---

## Rust Dependencies

The following is a summary of licenses used by Rust dependencies:

| License | Packages |
|---------|----------|
| MIT | axum, tokio, tower, hyper, rusqlite, include_dir, ssh2-config, and others |
| MIT OR Apache-2.0 | serde, chrono, clap, thiserror, rand, sha2, uuid, and others |
| Apache-2.0 | sync_wrapper |
| MPL-2.0 | option-ext |
| BSD-3-Clause | matchit (partial) |
| BSL-1.0 | ryu |
| Unicode-3.0 | unicode-ident |

For a complete list, run:
```bash
cd rust-sdk && cargo tree --prefix none --format '{l}:{p}' | sort -u
```

---

## TypeScript Dependencies

The following is a summary of licenses used by TypeScript dependencies:

| License | Packages |
|---------|----------|
| MIT | hono, @hono/node-server, better-sqlite3, csv-parse, csv-stringify, ssh-config, ssh2 |

For a complete list, run:
```bash
cd ts-sdk && npx license-checker --production
```

---

## Notes

- Most dependencies are dual-licensed under MIT OR Apache-2.0, allowing users to choose either license.
- MPL-2.0 (option-ext): File-level copyleft. No obligations unless you modify the source files directly.
- All listed licenses are compatible with this project's MIT license.
