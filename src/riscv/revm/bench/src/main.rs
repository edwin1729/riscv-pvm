// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use std::error::Error;
use std::path::Path;

use generate::handle_generate;

mod generate;

const DEFAULT_ROLLUP_ADDRESS: &str = "sr1UNDWPUYVeomgG15wn5jSw689EJ4RNnVQa";
const INBOX_FILE: &str = "inbox.json";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    handle_generate(DEFAULT_ROLLUP_ADDRESS, Path::new(INBOX_FILE))
}
