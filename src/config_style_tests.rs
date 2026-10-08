//! Tests of how the style layer reads the root plate's config keys. They lived in
//! `config.rs`'s suite until that module moved to `cce_core::config`; they test
//! `color` and `layout`, which stay here.

/// `style.surface.plate.root.*` is the only root-plate spelling: the
/// legacy `root plate.*` block is ignored, whether it stands beside the
/// canonical block or alone (its read-alias was removed 2026-09-06).
#[test]
fn test_plate_root_canonical_spelling() {
    // Global color state: serialize against the other reload_colors tests,
    // and fire both once-per-process live-config loads before our reload
    // so neither can rewrite the state mid-assert.
    let _guard = crate::color::test_color_state_lock();
    let _ = crate::color::root_plate_corner_radius();
    crate::layout::lazy_init_style_registry();

    let content = r##"
        style {
            surface {
                plate {
                    root corner_radius=(i64)17 {
                        menubar blur=(bool)true color=(rgba)"#1a1d26d0"
                    }
                }
                backplate corner_radius=(i64)9
            }
        }
    "##;
    crate::color::reload_colors(content);
    assert_eq!(crate::color::root_plate_corner_radius(), 17.0, "canonical read; legacy ignored");
    assert_eq!(crate::color::root_plate_menubar_blur(), true);

    // A legacy-only spelling no longer feeds the getter: the value from
    // the canonical load above stands.
    let legacy = r##"
        style {
            surface {
                backplate corner_radius=(i64)9
            }
        }
    "##;
    crate::color::reload_colors(legacy);
    assert_eq!(crate::color::root_plate_corner_radius(), 17.0, "legacy spelling is not read");
}

#[test]
fn test_root_plate_menubar_statusbar_styling() {
    // Global color state: serialize against the other reload_colors tests,
    // and fire both once-per-process live-config loads before our reload
    // so neither can rewrite the state mid-assert.
    let _guard = crate::color::test_color_state_lock();
    let _ = crate::color::root_plate_statusbar_blur();
    crate::layout::lazy_init_style_registry();

    let content = r##"
        style {
            surface {
                plate {
                    root blur=(f64)0.1 color=(rgba)"#5e657acf" corner_radius=(i64)12 {
                        menubar blur=(bool)true color=(rgba)"#1a1d26d0" text_color=(rgba)"#e2e4f0ff"
                    }
                }
                statusbar blur=(bool)false color=(rgba)"#12141cd0" text_color=(rgba)"#b5b9c8ff"
            }
            control {
                dropdown color=(rgba)"#08080cff"
            }
            data {
                textbox placeholder_text_color=(rgba)"#60606aff"
            }
        }
    "##;
    
    // Parse into json and set colors
    crate::color::reload_colors(content);

    // Verify values are parsed correctly through the root_plate_* getters.
    assert_eq!(crate::color::root_plate_menubar_blur(), true);
    
    let dd_color = crate::color::dropdown_background_color();
    assert!((dd_color[0] - crate::color::srgb_to_linear(8.0 / 255.0)).abs() < 0.0001);
    
    let placeholder_color = crate::color::textbox_placeholder_text_color();
    assert_eq!(placeholder_color, [0x60, 0x60, 0x6a]);
    assert_eq!(crate::color::root_plate_statusbar_blur(), false);

    // Colors are in sRGB converted to linear, let's verify text colors
    let menubar_txt = crate::color::root_plate_menubar_text_color();
    assert!(menubar_txt[0] > 0.0);
    let statusbar_txt = crate::color::root_plate_statusbar_text_color();
    assert!(statusbar_txt[0] > 0.0);
}
