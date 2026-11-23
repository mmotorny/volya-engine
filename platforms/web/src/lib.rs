// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya Engine for web browsers: the wasm entry point loaded by
//! `static/main.js`. Builds to nothing on other targets.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

/// Called by the web shell after the module is instantiated.
#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let status = document
        .get_element_by_id("status")
        .ok_or("missing #status")?;
    status.set_text_content(Some(&volya_engine::status_text()));
    status.set_attribute("data-state", "ready")?;
    Ok(())
}

/// [`volya_engine::test_api::status`], exposed to the web shell.
#[wasm_bindgen]
pub fn test_status() -> String {
    volya_engine::test_api::status()
}
