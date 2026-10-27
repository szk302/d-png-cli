// Diagnostics are large but only travel on error paths, where their detail
// (location, reference, candidates) is the point; boxing them would only
// complicate every signature.
#![allow(clippy::result_large_err)]

pub mod adapters;
pub mod app;
pub mod domain;

pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Licenses and notices for d-png-cli, its Cargo dependencies, and bundled works.
pub const BUNDLED_LICENSES: &str = concat!(
    "--- d-png-cli original code (MIT) ---\n",
    include_str!("../LICENSE"),
    "\n\n--- Third-party notices ---\n",
    include_str!("../THIRD_PARTY_NOTICES.md"),
    "\n\n--- Bundled draw.io renderer ---\n",
    include_str!("../assets/drawio/README.md"),
    "\n\n--- musl libc 1.2.5 (statically linked into the Linux release binaries) ---\n",
    include_str!("../assets/licenses/musl-1.2.5-COPYRIGHT.txt"),
    "\n\n--- Chromium resampling (Copyright 2011, 2012 The Chromium Authors) ---\n",
    include_str!("../assets/licenses/chromium-BSD-3-Clause.txt"),
    "\n\n--- DOMPurify-Apache-2.0.txt ---\n",
    include_str!("../assets/drawio/licenses/DOMPurify-Apache-2.0.txt"),
    "\n\n--- drawio-Apache-2.0.txt ---\n",
    include_str!("../assets/drawio/licenses/drawio-Apache-2.0.txt"),
    "\n\n--- hachure-fill-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/hachure-fill-MIT.txt"),
    "\n\n--- pako-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/pako-MIT.txt"),
    "\n\n--- pako-zlib.txt ---\n",
    include_str!("../assets/drawio/licenses/pako-zlib.txt"),
    "\n\n--- path-data-parser-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/path-data-parser-MIT.txt"),
    "\n\n--- points-on-curve-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/points-on-curve-MIT.txt"),
    "\n\n--- points-on-path-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/points-on-path-MIT.txt"),
    "\n\n--- roughjs-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/roughjs-MIT.txt"),
    "\n\n--- spin-MIT.txt ---\n",
    include_str!("../assets/drawio/licenses/spin-MIT.txt"),
    "\n\n--- Cargo dependencies ---\n",
    include_str!("../assets/licenses/cargo-dependencies.txt"),
);
