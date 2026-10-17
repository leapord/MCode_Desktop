//! 主题 — 颜色语义 1:1 对齐 desktop-prototype/css/app.css 的设计令牌

use gpui::{rgb, Rgba};

pub const BG_WINDOW: Rgba = rgb(0x111114);
pub const BG_SIDE: Rgba = rgb(0x17171b);
pub const BG_MAIN: Rgba = rgb(0x1c1c21);
pub const BG_PANEL: Rgba = rgb(0x222228);
pub const BG_HOVER: Rgba = rgb(0x2a2a31);
pub const BG_ACTIVE: Rgba = rgb(0x32323b);
pub const BORDER: Rgba = rgb(0x2e2e36);
pub const BORDER_STRONG: Rgba = rgb(0x3d3d47);

pub const TEXT_1: Rgba = rgb(0xe8e8ec);
pub const TEXT_2: Rgba = rgb(0xa8a8b3);
pub const TEXT_3: Rgba = rgb(0x6d6d78);

pub const ACCENT: Rgba = rgb(0xd97757);
pub const BLUE: Rgba = rgb(0x5b9bd5);
pub const GREEN: Rgba = rgb(0x6fbf73);
pub const YELLOW: Rgba = rgb(0xd9a144);
pub const RED: Rgba = rgb(0xe05555);
pub const PURPLE: Rgba = rgb(0xa077d4);

/// 权限模式语义色（红/黄/蓝/紫，与原型与 CLI 状态栏一致）
pub fn mode_color(m: mcode_core::permission::Mode) -> Rgba {
    use mcode_core::permission::Mode::*;
    match m {
        Manual => YELLOW,
        Auto => BLUE,
        Plan => PURPLE,
        Passon => RED,
    }
}
