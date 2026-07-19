use crate::{
    errors::ApiError,
    models::{EmailBranding, UpdateEmailBrandingRequest},
};

/// Branding plus whatever's resolved at render time (currently just the
/// logo's absolute CDN URL, resolved once by `render_template` from
/// `branding.logo_file_id` — templates never construct URLs themselves).
pub struct EmailTheme<'a> {
    pub branding: &'a EmailBranding,
    pub logo_url: Option<String>,
}

impl<'a> EmailTheme<'a> {
    pub fn new(branding: &'a EmailBranding, logo_url: Option<String>) -> Self {
        Self { branding, logo_url }
    }
}

type ColorFieldAccessor = fn(&UpdateEmailBrandingRequest) -> &str;

const COLOR_FIELDS: &[(&str, ColorFieldAccessor)] = &[
    ("header_background_color", |b| &b.header_background_color),
    ("header_text_color", |b| &b.header_text_color),
    ("page_background_color", |b| &b.page_background_color),
    ("panel_background_color", |b| &b.panel_background_color),
    ("text_color", |b| &b.text_color),
    ("heading_color", |b| &b.heading_color),
    ("link_color", |b| &b.link_color),
    ("accent_color", |b| &b.accent_color),
    ("button_background_color", |b| &b.button_background_color),
    ("button_text_color", |b| &b.button_text_color),
];

/// Allow-list of email-safe font stacks. `font_family` fields store the key
/// (e.g. `"roboto_sans"`), never the raw CSS stack — free-text font input
/// would be spliced unescaped into a raw `<style>` block (see spec 013).
const FONT_STACKS: &[(&str, &str)] = &[
    ("roboto_sans", "Roboto,Arial,Helvetica,sans-serif"),
    (
        "system_sans",
        "-apple-system,'Segoe UI',Roboto,Helvetica,Arial,sans-serif",
    ),
    ("georgia_serif", "Georgia,'Times New Roman',serif"),
    ("monospace", "'Courier New',monospace"),
];

const FONT_SIZE_SCALES: &[&str] = &["small", "medium", "large"];
const CORNER_STYLES: &[&str] = &["sharp", "rounded", "soft"];

pub fn validate_branding_input(input: &UpdateEmailBrandingRequest) -> Result<(), ApiError> {
    for (_, accessor) in COLOR_FIELDS {
        if !is_valid_hex_color(accessor(input)) {
            return Err(ApiError::BadRequest);
        }
    }

    if resolve_font_stack(&input.heading_font_family).is_none()
        || resolve_font_stack(&input.body_font_family).is_none()
    {
        return Err(ApiError::BadRequest);
    }

    if !FONT_SIZE_SCALES.contains(&input.font_size_scale.as_str()) {
        return Err(ApiError::BadRequest);
    }

    if !CORNER_STYLES.contains(&input.corner_style.as_str()) {
        return Err(ApiError::BadRequest);
    }

    validate_text_field(&input.brand_name, 100)?;
    validate_text_field(&input.tagline, 150)?;
    validate_text_field(&input.footer_text, 200)?;

    Ok(())
}

fn validate_text_field(value: &str, max_len: usize) -> Result<(), ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max_len {
        return Err(ApiError::BadRequest);
    }
    Ok(())
}

pub fn is_valid_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

pub fn resolve_font_stack(key: &str) -> Option<&'static str> {
    FONT_STACKS
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, stack)| *stack)
}

struct FontSizes {
    brand_px: u32,
    eyebrow_px: u32,
    heading_px: u32,
    body_px: u32,
    footer_px: u32,
}

fn font_sizes(scale: &str) -> FontSizes {
    match scale {
        "small" => FontSizes {
            brand_px: 26,
            eyebrow_px: 11,
            heading_px: 26,
            body_px: 14,
            footer_px: 12,
        },
        "large" => FontSizes {
            brand_px: 34,
            eyebrow_px: 13,
            heading_px: 34,
            body_px: 18,
            footer_px: 14,
        },
        _ => FontSizes {
            brand_px: 30,
            eyebrow_px: 12,
            heading_px: 30,
            body_px: 16,
            footer_px: 13,
        },
    }
}

struct CornerRadii {
    shell_px: u32,
    button_px: u32,
    callout_px: u32,
}

fn corner_radii(style: &str) -> CornerRadii {
    match style {
        "sharp" => CornerRadii {
            shell_px: 0,
            button_px: 0,
            callout_px: 0,
        },
        "rounded" => CornerRadii {
            shell_px: 10,
            button_px: 6,
            callout_px: 4,
        },
        _ => CornerRadii {
            shell_px: 18,
            button_px: 8,
            callout_px: 6,
        },
    }
}

fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Lightens `hex` toward white by `white_fraction` (0.0-1.0), used to derive
/// the callout background tint from `accent_color` instead of storing an
/// eleventh color field nobody would tune independently of the accent it's
/// paired with.
fn blend_with_white(hex: &str, white_fraction: f32) -> String {
    let Some((r, g, b)) = hex_to_rgb(hex) else {
        return "#f7ecec".to_string();
    };
    let mix = |channel: u8| -> u8 {
        let blended = f32::from(channel) * (1.0 - white_fraction) + 255.0 * white_fraction;
        blended.round().clamp(0.0, 255.0) as u8
    };
    format!("#{:02x}{:02x}{:02x}", mix(r), mix(g), mix(b))
}

pub fn stylesheet(branding: &EmailBranding) -> String {
    let heading_font = resolve_font_stack(&branding.heading_font_family)
        .unwrap_or("Roboto,Arial,Helvetica,sans-serif");
    let body_font = resolve_font_stack(&branding.body_font_family)
        .unwrap_or("Roboto,Arial,Helvetica,sans-serif");
    let sizes = font_sizes(&branding.font_size_scale);
    let radii = corner_radii(&branding.corner_style);
    let callout_background = blend_with_white(&branding.accent_color, 0.93);

    format!(
        r#"
body{{margin:0;padding:0;background:{page_bg};color:{text};font-family:{body_font}}}
.bg{{background:{page_bg};padding:24px}}
.shell{{max-width:640px;margin:0 auto}}
.header{{background:{header_bg};padding:30px 32px;color:{header_text};border-radius:{shell_r}px {shell_r}px 0 0}}
.header img{{max-height:48px}}
.brand{{font-size:{brand_size}px;line-height:1.1;font-weight:700;letter-spacing:.02em;font-family:{heading_font}}}
.eyebrow{{font-size:{eyebrow_size}px;letter-spacing:.16em;text-transform:uppercase;opacity:.88;margin-top:8px}}
.panel{{background:{panel_bg};padding:36px 32px;border-left:1px solid #d9dce5;border-right:1px solid #d9dce5}}
.panel h1{{margin:0 0 18px;color:{heading};font-size:{heading_size}px;line-height:1.2;font-weight:700;font-family:{heading_font}}}
.panel p{{font-size:{body_size}px;line-height:1.6;margin:0 0 16px;color:{text}}}
.panel a{{color:{link}}}
.panel strong{{color:{accent}}}
.callout{{background:{callout_bg};border-left:4px solid {accent};padding:14px 16px;margin:18px 0;border-radius:{callout_r}px}}
.callout p:last-child{{margin-bottom:0}}
.button{{display:inline-block;padding:12px 18px;background:{button_bg};color:{button_text} !important;text-decoration:none;border-radius:{button_r}px;font-weight:700}}
.footer{{background:#f7f8fb;padding:20px 32px;border:1px solid #d9dce5;border-top:0;border-radius:0 0 {shell_r}px {shell_r}px;color:#5d6472}}
.footer p{{margin:0;font-size:{footer_size}px;line-height:1.6}}
.footer a{{color:{link}}}
.footer-link{{margin-top:8px !important}}
.preheader{{display:none!important;visibility:hidden;opacity:0;color:transparent;height:0;width:0;overflow:hidden}}
@media only screen and (max-width:640px){{.bg{{padding:12px}}.header,.panel,.footer{{padding-left:22px;padding-right:22px}}.panel{{padding-top:30px;padding-bottom:30px}}.panel h1{{font-size:{heading_size_mobile}px}}}}
"#,
        page_bg = branding.page_background_color,
        text = branding.text_color,
        body_font = body_font,
        header_bg = branding.header_background_color,
        header_text = branding.header_text_color,
        shell_r = radii.shell_px,
        brand_size = sizes.brand_px,
        eyebrow_size = sizes.eyebrow_px,
        heading_font = heading_font,
        panel_bg = branding.panel_background_color,
        heading = branding.heading_color,
        heading_size = sizes.heading_px,
        body_size = sizes.body_px,
        link = branding.link_color,
        accent = branding.accent_color,
        callout_bg = callout_background,
        callout_r = radii.callout_px,
        button_bg = branding.button_background_color,
        button_text = branding.button_text_color,
        button_r = radii.button_px,
        footer_size = sizes.footer_px,
        heading_size_mobile = sizes.heading_px.saturating_sub(4),
    )
}
