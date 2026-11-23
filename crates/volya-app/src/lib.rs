// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Volya engine application entry point.

/// Engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Text shown by the web shell once the engine has started.
pub fn status_text() -> String {
    format!("Volya {VERSION}")
}

#[cfg(target_arch = "wasm32")]
mod web {
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
        status.set_text_content(Some(&super::status_text()));
        status.set_attribute("data-state", "ready")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_contains_version() {
        assert_eq!(status_text(), format!("Volya {VERSION}"));
    }

    #[cfg(target_arch = "wasm32")]
    mod browser {
        use wasm_bindgen_test::*;

        wasm_bindgen_test_configure!(run_in_browser);

        #[wasm_bindgen_test]
        fn has_document() {
            assert!(web_sys::window().and_then(|w| w.document()).is_some());
        }
    }
}
