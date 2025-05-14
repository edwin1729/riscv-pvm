// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use crate::Result;
use std::path::Path;

pub fn handle_results(
    _inbox: Box<Path>,
    _all_logs: Vec<Box<Path>>,
    _expected_transfers: usize,
) -> Result<()> {
    unimplemented!("results reporting for TPS benchmark")
}
