// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

fn main() {
    println!("cargo:rerun-if-env-changed=INBOX_FILE");
}
