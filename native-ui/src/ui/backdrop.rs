//! Native Windows 11 window materials, set through DWM.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Acrylic,
    Mica,
    Tabbed,
    Solid,
}

impl Material {
    pub const ALL: [Self; 4] = [Self::Acrylic, Self::Mica, Self::Tabbed, Self::Solid];

    pub fn key(self) -> &'static str {
        match self {
            Self::Acrylic => "acrylic",
            Self::Mica => "mica",
            Self::Tabbed => "tabbed",
            Self::Solid => "solid",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Acrylic => "Acrylic",
            Self::Mica => "Mica",
            Self::Tabbed => "Mica Alt",
            Self::Solid => "Solid",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|material| material.key() == key)
            .unwrap_or(Self::Acrylic)
    }

    /// Window tint alpha for this material at a glass strength of 0..=1.
    /// Higher strength lets more of the desktop through.
    pub fn tint_alpha(self, strength: f32) -> u8 {
        if self == Self::Solid {
            return 255;
        }
        (245.0 - 175.0 * strength.clamp(0.0, 1.0)) as u8
    }
}

/// Drop the system-menu style so the extended frame cannot draw its own caption
/// buttons under ours. The window library restores the bit on some state
/// changes, so this is cheap enough to call every frame.
#[cfg(windows)]
pub fn hide_system_buttons(window: isize) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowLongPtrW(window: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(window: isize, index: i32, value: isize) -> isize;
        fn SetWindowPos(
            window: isize,
            after: isize,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }
    const STYLE: i32 = -16;
    const SYSTEM_MENU: isize = 0x0008_0000;
    // NOSIZE | NOMOVE | NOZORDER | NOACTIVATE | FRAMECHANGED
    const FRAME_CHANGED_ONLY: u32 = 0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020;
    // SAFETY: a style-bit update on this process's own live window.
    unsafe {
        let style = GetWindowLongPtrW(window, STYLE);
        if style & SYSTEM_MENU != 0 {
            SetWindowLongPtrW(window, STYLE, style & !SYSTEM_MENU);
            SetWindowPos(window, 0, 0, 0, 0, 0, FRAME_CHANGED_ONLY);
        }
    }
}

#[cfg(not(windows))]
pub fn hide_system_buttons(_window: isize) {}

/// Apply the material. Returns false when this Windows build cannot do it, in
/// which case the caller must paint a solid window.
#[cfg(windows)]
pub fn apply(window: isize, material: Material, dark: bool) -> bool {
    use std::ffi::c_void;

    #[repr(C)]
    struct Margins {
        left: i32,
        right: i32,
        top: i32,
        bottom: i32,
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            window: isize,
            attribute: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
        fn DwmExtendFrameIntoClientArea(window: isize, margins: *const Margins) -> i32;
    }

    const USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const WINDOW_CORNER_PREFERENCE: u32 = 33;
    const SYSTEMBACKDROP_TYPE: u32 = 38;
    const CAPTION_COLOR: u32 = 35;
    const COLOR_NONE: i32 = -2; // 0xFFFFFFFE

    let set = |attribute: u32, value: i32| -> bool {
        // SAFETY: `window` is the live HWND of this process's main window and
        // the value pointer is valid for the stated four bytes.
        unsafe { DwmSetWindowAttribute(window, attribute, (&raw const value).cast(), 4) >= 0 }
    };

    hide_system_buttons(window);

    set(USE_IMMERSIVE_DARK_MODE, i32::from(dark));
    set(WINDOW_CORNER_PREFERENCE, 2);
    // Otherwise the accent-on-title-bar setting tints a strip under our own bar.
    set(CAPTION_COLOR, COLOR_NONE);
    let solid = material == Material::Solid;
    let margin = if solid { 0 } else { -1 };
    let margins = Margins {
        left: margin,
        right: margin,
        top: margin,
        bottom: margin,
    };
    // SAFETY: as above; `margins` outlives the call.
    let extended = unsafe { DwmExtendFrameIntoClientArea(window, &margins) >= 0 };
    let kind = match material {
        Material::Solid => 1,
        Material::Mica => 2,
        Material::Acrylic => 3,
        Material::Tabbed => 4,
    };
    let applied = set(SYSTEMBACKDROP_TYPE, kind);
    !solid && extended && applied
}

#[cfg(not(windows))]
pub fn apply(_window: isize, _material: Material, _dark: bool) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::Material;

    #[test]
    fn solid_is_opaque_and_stronger_glass_is_more_see_through() {
        assert_eq!(Material::Solid.tint_alpha(1.0), 255);
        assert!(Material::Acrylic.tint_alpha(0.9) < Material::Acrylic.tint_alpha(0.1));
        assert_eq!(Material::from_key("nonsense"), Material::Acrylic);
    }
}
