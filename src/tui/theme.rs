use ratatui::prelude::*;

/// Modern theme system for NaviPod inspired by Kubernetes and modern TUI apps
#[derive(Clone, Debug)]
pub struct NaviTheme {
    // Core colors
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    
    // Background hierarchy
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_accent: Color,
    
    // Text hierarchy
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_inverse: Color,
    
    // Interactive states
    pub selected: Color,
    pub hover: Color,
    pub focus: Color,
    pub disabled: Color,
    
    // Border and divider
    pub border: Color,
    pub border_focus: Color,
    pub divider: Color,
}

impl Default for NaviTheme {
    fn default() -> Self {
        Self::kubernetes_blue()
    }
}

impl NaviTheme {
    /// Kubernetes-inspired blue theme (primary) - optimized for readability
    #[allow(clippy::unreadable_literal)]  // Color codes are more readable as hex without underscores
    pub fn kubernetes_blue() -> Self {
        Self {
            // Kubernetes blue palette - slightly more muted for better readability
            primary: Color::from_u32(0x2563EB),      // More readable blue
            secondary: Color::from_u32(0x3B82F6),    // Lighter blue
            accent: Color::from_u32(0x06B6D4),       // Cyan (easier on eyes than teal)
            success: Color::from_u32(0x10B981),      // Emerald green
            warning: Color::from_u32(0xF59E0B),      // Amber
            error: Color::from_u32(0xEF4444),        // Red
            info: Color::from_u32(0x3B82F6),         // Blue
            
            // Dark theme backgrounds - warmer and more consistent
            bg_primary: Color::from_u32(0x0F172A),   // Slate 900 (warmer dark)
            bg_secondary: Color::from_u32(0x1E293B), // Slate 800 
            bg_tertiary: Color::from_u32(0x334155),  // Slate 700 (card backgrounds)
            bg_accent: Color::from_u32(0x475569),    // Slate 600 (accent backgrounds)
            
            // Text colors - improved contrast
            text_primary: Color::from_u32(0xF8FAFC),   // Slate 50 (very light)
            text_secondary: Color::from_u32(0xCBD5E1), // Slate 300 (readable secondary)
            text_muted: Color::from_u32(0x94A3B8),     // Slate 400 (muted but readable)
            text_inverse: Color::from_u32(0x0F172A),   // Dark on light
            
            // Interactive states - more subtle
            selected: Color::from_u32(0x059669),    // Emerald 600
            hover: Color::from_u32(0x475569),      // Slate 600
            focus: Color::from_u32(0x2563EB),      // Blue 600
            disabled: Color::from_u32(0x64748B),   // Slate 500
            
            // Borders - more visible
            border: Color::from_u32(0x475569),       // Slate 600 (more visible)
            border_focus: Color::from_u32(0x2563EB), // Blue 600
            divider: Color::from_u32(0x334155),      // Slate 700
        }
    }
    
    /// Alternative green theme
    #[allow(clippy::unreadable_literal)]  // Color codes are more readable as hex without underscores
    pub fn kubernetes_green() -> Self {
        let mut theme = Self::kubernetes_blue();
        theme.primary = Color::from_u32(0x00D4AA);   // Kubernetes teal
        theme.secondary = Color::from_u32(0x26A69A); // Teal 600
        theme.accent = Color::from_u32(0x326CE5);    // Blue accent
        theme.selected = Color::from_u32(0x00BCD4);  // Cyan
        theme
    }
    
    /// Get color for Kubernetes resource status
    pub fn status_color(&self, status: ResourceStatus) -> Color {
        match status {
            ResourceStatus::Running | ResourceStatus::Ready => self.success,
            ResourceStatus::Pending | ResourceStatus::Updating => self.warning,
            ResourceStatus::Failed | ResourceStatus::Error => self.error,
            ResourceStatus::Unknown => self.text_muted,
        }
    }
    
    /// Get appropriate text style for content type
    pub fn text_style(&self, content_type: TextType) -> Style {
        match content_type {
            TextType::Title => Style::default()
                .fg(self.text_primary)
                .add_modifier(Modifier::BOLD),
            TextType::Subtitle => Style::default()
                .fg(self.text_secondary)
                .add_modifier(Modifier::BOLD),
            TextType::Body => Style::default()
                .fg(self.text_primary),
            TextType::Caption => Style::default()
                .fg(self.text_muted),
            TextType::Success => Style::default()
                .fg(self.success)
                .add_modifier(Modifier::BOLD),
            TextType::Warning => Style::default()
                .fg(self.warning)
                .add_modifier(Modifier::BOLD),
            TextType::Error => Style::default()
                .fg(self.error)
                .add_modifier(Modifier::BOLD),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceStatus {
    Running,
    Ready,
    Pending,
    Updating,
    Failed,
    Error,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub enum TextType {
    Title,
    Subtitle,
    Body,
    Caption,
    Success,
    Warning,
    Error,
}

/// Unicode symbols for modern TUI design
pub struct Symbols;

impl Symbols {
    // Status indicators
    pub const RUNNING: &'static str = "●";
    pub const PENDING: &'static str = "◐";
    pub const ERROR: &'static str = "✗";
    pub const SUCCESS: &'static str = "✓";
    pub const WARNING: &'static str = "⚠";
    pub const UNKNOWN: &'static str = "?";
    
    // Kubernetes resources
    pub const REPLICASET: &'static str = "⚙";
    pub const POD: &'static str = "□";
    pub const CONTAINER: &'static str = "▢";
    pub const SERVICE: &'static str = "🌐";
    pub const INGRESS: &'static str = "→";
    
    // UI elements
    pub const ARROW_RIGHT: &'static str = "→";
    pub const ARROW_DOWN: &'static str = "↓";
    pub const CHEVRON_RIGHT: &'static str = "›";
    pub const CHEVRON_DOWN: &'static str = "‹";
    pub const BULLET: &'static str = "•";
    pub const DOT: &'static str = "·";
    pub const FOLDER: &'static str = "📁";
    pub const SETTINGS: &'static str = "⚙";
    
    // Progress and loading
    pub const SPINNER: [&'static str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    pub const PROGRESS_EMPTY: &'static str = "░";
    pub const PROGRESS_FILL: &'static str = "█";
    
    // Borders (rounded)
    pub const BORDER_TOP_LEFT: &'static str = "╭";
    pub const BORDER_TOP_RIGHT: &'static str = "╮";
    pub const BORDER_BOTTOM_LEFT: &'static str = "╰";
    pub const BORDER_BOTTOM_RIGHT: &'static str = "╯";
    pub const BORDER_HORIZONTAL: &'static str = "─";
    pub const BORDER_VERTICAL: &'static str = "│";
}

/// UI constants for consistent sizing across all modules
pub struct UiConstants;

impl UiConstants {
    /// Standard card height for list items
    pub const CARD_HEIGHT: u16 = 4;
    
    /// Standard header height
    pub const HEADER_HEIGHT: u16 = 3;
    
    /// Standard footer height  
    pub const FOOTER_HEIGHT: u16 = 2;
    
    /// Standard progress bar width for health indicators
    pub const PROGRESS_BAR_WIDTH: usize = 8;
    
    /// Standard details panel width
    pub const DETAILS_PANEL_WIDTH: u16 = 40;
    
    /// Standard minimum list width
    pub const MIN_LIST_WIDTH: u16 = 65;
    
    /// Standard icon column width
    pub const ICON_COLUMN_WIDTH: u16 = 20;
    
    /// Standard actions column width
    pub const ACTIONS_COLUMN_WIDTH: u16 = 30;
}

/// Helper functions for common UI patterns
pub struct UiHelpers;

impl UiHelpers {
    /// Create a status indicator with symbol and color
    pub fn status_indicator(status: ResourceStatus, theme: &NaviTheme) -> (String, Style) {
        let (symbol, color) = match status {
            ResourceStatus::Running | ResourceStatus::Ready => {
                (Symbols::SUCCESS, theme.success)
            }
            ResourceStatus::Pending | ResourceStatus::Updating => {
                (Symbols::PENDING, theme.warning)
            }
            ResourceStatus::Failed | ResourceStatus::Error => {
                (Symbols::ERROR, theme.error)
            }
            ResourceStatus::Unknown => {
                (Symbols::UNKNOWN, theme.text_muted)
            }
        };
        
        (symbol.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD))
    }
    
    /// Create a health-based progress bar with color gradient
    pub fn health_progress_bar(current: usize, total: usize, width: usize, theme: &NaviTheme) -> (String, Color) {
        if total == 0 {
            return (Symbols::PROGRESS_EMPTY.repeat(width), theme.text_muted);
        }
        
        let filled = (current * width) / total;
        let empty = width - filled;
        let health_ratio = current as f32 / total as f32;
        
        // Health-based color gradient: green (100%) -> yellow (75%) -> orange (50%) -> red (0%)
        #[allow(clippy::unreadable_literal)]  // Color codes are more readable as hex without underscores
        let color = if health_ratio >= 1.0 {
            theme.success // Perfect health: emerald green
        } else if health_ratio >= 0.75 {
            Color::from_u32(0x84CC16) // Lime 500 - mostly healthy
        } else if health_ratio >= 0.5 {
            theme.warning // Amber - degraded but functional
        } else if health_ratio >= 0.25 {
            Color::from_u32(0xF97316) // Orange 500 - unhealthy  
        } else {
            theme.error // Red - critical/failing
        };
        
        let filled_bar = Symbols::PROGRESS_FILL.repeat(filled);
        let empty_bar = Symbols::PROGRESS_EMPTY.repeat(empty);
        let bar = format!("{filled_bar}{empty_bar}");
        
        (bar, color)
    }
    
    /// Create a simple progress bar (for non-health metrics)
    pub fn progress_bar(current: usize, total: usize, width: usize) -> String {
        if total == 0 {
            return Symbols::PROGRESS_EMPTY.repeat(width);
        }
        
        let filled = (current * width) / total;
        let empty = width - filled;
        
        let filled_bar = Symbols::PROGRESS_FILL.repeat(filled);
        let empty_bar = Symbols::PROGRESS_EMPTY.repeat(empty);
        format!("{filled_bar}{empty_bar}")
    }
    
    /// Create a resource type indicator
    pub fn resource_icon(resource_type: &str) -> &'static str {
        match resource_type.to_lowercase().as_str() {
            "replicaset" => Symbols::REPLICASET,
            "pod" => Symbols::POD,
            "container" => Symbols::CONTAINER,
            "service" => Symbols::SERVICE,
            "ingress" => Symbols::INGRESS,
            _ => Symbols::BULLET,
        }
    }
    
    /// Safely parse a numeric string value with logging for debugging
    pub fn safe_parse_i32(value: &str, _context: &str) -> i32 {
        value.parse::<i32>().unwrap_or({
            // In a production environment, you might want to log this
            // tracing::debug!("Failed to parse {} as i32 in context: {}", value, context);
            0
        })
    }
    
    /// Safely cast usize to u16 with bounds checking
    pub fn safe_cast_u16(value: usize, _context: &str) -> u16 {
        if value > u16::MAX as usize {
            // tracing::warn!("Value {} exceeds u16::MAX in context: {}, clamping to u16::MAX", value, context);
            u16::MAX
        } else {
            value as u16
        }
    }
}