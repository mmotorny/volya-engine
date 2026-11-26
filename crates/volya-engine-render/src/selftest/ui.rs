// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The UI layer's scenes: quads, textures by every path, the gamma ramp,
//! the scaled fit and video frames.

use super::{Make, Outcome, capture, ensure};
use crate::DrawList;

pub async fn empty_list_clears(make: Make<'_>) -> Outcome {
    let mut renderer = make();
    let image = capture(&mut renderer, &DrawList::default(), 8, 6).await?;
    ensure!(
        image.chunks(4).all(|p| p == [0, 0, 0, 255]),
        "not black: {:?}",
        &image[..8]
    );
    Ok(String::new())
}
