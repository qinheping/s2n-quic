// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    fill_size: u32,
    comp_size: u32,
    frame_size: u32,
    frame_headroom: u32,
    flags: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fill_size: 2048,
            comp_size: 2048,
            frame_size: 4096,
            frame_headroom: 0,
            flags: 0,
        }
    }
}

pub struct Umem {
    fill_save: (),
    comp_save: (),
    area: Arc<[u8]>,
}

impl Umem {}
