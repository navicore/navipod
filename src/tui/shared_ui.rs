use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

/// Shared UI components to reduce code duplication across modern UI modules
pub struct SharedComponents;

impl SharedComponents {
    /// Renders a modal overlay with centered content
    pub fn render_modal_overlay(
        f: &mut Frame,
        title: &str,
        content: Paragraph,
        theme: &NaviTheme,
    ) {
        let popup_area = Self::centered_rect(60, 40, f.area());
        
        // Clear the area first
        f.render_widget(Clear, popup_area);
        
        // Render the modal block
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focus))
            .style(Style::default().bg(theme.bg_secondary));
        
        f.render_widget(block, popup_area);
        
        // Render content inside the modal
        let inner = popup_area.inner(Margin::new(2, 1));
        f.render_widget(content, inner);
    }
    
    /// Renders a health bar indicator with percentage and color coding
    pub fn render_health_bar(
        value: i32,
        max_value: i32,
        width: u16,
        theme: &NaviTheme,
    ) -> String {
        let percentage = if max_value > 0 {
            (value * 100) / max_value
        } else {
            0
        };
        
        let bar_width = if width > 10 { width - 10 } else { 1 } as usize;
        let filled = ((percentage as usize * bar_width) / 100).min(bar_width);
        let empty = bar_width.saturating_sub(filled);
        
        let bar_color = if percentage >= 80 {
            "🟩"
        } else if percentage >= 60 {
            "🟨"
        } else if percentage >= 40 {
            "🟧"
        } else {
            "🟥"
        };
        
        format!(
            "{} {:3}%",
            bar_color.repeat(filled.max(1)) + &"⬜".repeat(empty),
            percentage
        )
    }
    
    /// Renders a status indicator with appropriate symbol and color
    pub fn render_status_indicator(
        status: &ResourceStatus,
        theme: &NaviTheme,
    ) -> (String, Color) {
        match status {
            ResourceStatus::Running => (Symbols::STATUS_RUNNING.to_string(), theme.success),
            ResourceStatus::Pending => (Symbols::STATUS_PENDING.to_string(), theme.warning),
            ResourceStatus::Failed => (Symbols::STATUS_ERROR.to_string(), theme.error),
            ResourceStatus::Unknown => (Symbols::STATUS_UNKNOWN.to_string(), theme.text_muted),
        }
    }
    
    /// Creates a centered rectangle for modals and popups
    pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
        
        Layout::horizontal([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
    }
    
    /// Renders a standard header layout with title, context, and actions
    pub fn render_standard_header(
        f: &mut Frame,
        area: Rect,
        title: &str,
        context_info: Option<&str>,
        actions_info: Option<&str>,
        theme: &NaviTheme,
    ) {
        let header_chunks = Layout::horizontal([
            Constraint::Length(25),  // Title
            Constraint::Min(0),      // Context info (flexible)
            Constraint::Length(30),  // Actions
        ]).split(area);
        
        // Title
        let title_widget = Paragraph::new(title)
            .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
            .block(Block::default().borders(Borders::NONE));
        f.render_widget(title_widget, header_chunks[0]);
        
        // Context info
        if let Some(context) = context_info {
            let context_widget = Paragraph::new(context)
                .style(theme.text_style(TextType::Body).bg(theme.bg_primary))
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::NONE));
            f.render_widget(context_widget, header_chunks[1]);
        }
        
        // Actions
        if let Some(actions) = actions_info {
            let actions_widget = Paragraph::new(actions)
                .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
                .alignment(Alignment::Right)
                .block(Block::default().borders(Borders::NONE));
            f.render_widget(actions_widget, header_chunks[2]);
        }
    }
    
    /// Renders a standard footer with help text
    pub fn render_standard_footer(
        f: &mut Frame,
        area: Rect,
        help_text: &str,
        theme: &NaviTheme,
    ) {
        let footer = Paragraph::new(help_text)
            .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::NONE));
        f.render_widget(footer, area);
    }
}

/// Performance optimization: UI cache for expensive computations
#[derive(Default, Clone)]
pub struct UiCache {
    pub last_filter: String,
    pub filtered_indices: Vec<usize>,
    pub last_computed_stats: Option<String>,
    pub stats_generation: u64,
}

impl UiCache {
    /// Updates the filter cache if the filter has changed
    pub fn update_filter_cache<T, F>(&mut self, filter: &str, items: &[T], filter_fn: F) 
    where
        F: Fn(&T, &str) -> bool,
    {
        if self.last_filter != filter {
            self.filtered_indices.clear();
            
            if filter.is_empty() {
                // No filter: include all items
                self.filtered_indices.extend(0..items.len());
            } else {
                // Apply filter
                for (index, item) in items.iter().enumerate() {
                    if filter_fn(item, filter) {
                        self.filtered_indices.push(index);
                    }
                }
            }
            
            self.last_filter = filter.to_string();
        }
    }
    
    /// Gets the cached filtered indices
    pub fn get_filtered_indices(&self) -> &[usize] {
        &self.filtered_indices
    }
    
    /// Updates computed statistics cache
    pub fn update_stats_cache(&mut self, stats: String, generation: u64) {
        if self.stats_generation != generation {
            self.last_computed_stats = Some(stats);
            self.stats_generation = generation;
        }
    }
    
    /// Gets cached statistics if available
    pub fn get_cached_stats(&self) -> Option<&String> {
        self.last_computed_stats.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::prelude::Rect;
    
    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = SharedComponents::centered_rect(50, 40, area);
        
        // Should be centered with 50% width and 40% height
        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 20);
        assert_eq!(centered.x, 25);
        assert_eq!(centered.y, 15);
    }
    
    #[test]
    fn test_health_bar_rendering() {
        let theme = NaviTheme::default();
        
        // Test 100% health
        let bar = SharedComponents::render_health_bar(100, 100, 20, &theme);
        assert!(bar.contains("100%"));
        assert!(bar.contains("🟩"));
        
        // Test 50% health
        let bar = SharedComponents::render_health_bar(50, 100, 20, &theme);
        assert!(bar.contains("50%"));
        assert!(bar.contains("🟧"));
        
        // Test 0% health
        let bar = SharedComponents::render_health_bar(0, 100, 20, &theme);
        assert!(bar.contains("0%"));
        assert!(bar.contains("🟥"));
    }
    
    #[test]
    fn test_status_indicator() {
        let theme = NaviTheme::default();
        
        let (symbol, color) = SharedComponents::render_status_indicator(&ResourceStatus::Running, &theme);
        assert!(!symbol.is_empty());
        assert_eq!(color, theme.success);
        
        let (symbol, color) = SharedComponents::render_status_indicator(&ResourceStatus::Failed, &theme);
        assert!(!symbol.is_empty());
        assert_eq!(color, theme.error);
    }
    
    #[test]
    fn test_ui_cache_filter_functionality() {
        let mut cache = UiCache::default();
        let items = vec!["apple", "banana", "cherry", "date"];
        
        // Test initial filter
        cache.update_filter_cache("a", &items, |item, filter| item.contains(filter));
        let indices = cache.get_filtered_indices();
        assert_eq!(indices.len(), 3); // apple, banana, date
        assert_eq!(cache.last_filter, "a");
        
        // Test filter change
        cache.update_filter_cache("ch", &items, |item, filter| item.contains(filter));
        let indices = cache.get_filtered_indices();
        assert_eq!(indices.len(), 1); // cherry
        assert_eq!(cache.last_filter, "ch");
        
        // Test same filter (should not recompute)
        let old_indices = indices.to_vec();
        cache.update_filter_cache("ch", &items, |item, filter| item.contains(filter));
        let new_indices = cache.get_filtered_indices();
        assert_eq!(old_indices, new_indices);
    }
    
    #[test]
    fn test_ui_cache_stats_functionality() {
        let mut cache = UiCache::default();
        
        // Test initial stats
        cache.update_stats_cache("Initial stats".to_string(), 1);
        assert_eq!(cache.get_cached_stats(), Some(&"Initial stats".to_string()));
        assert_eq!(cache.stats_generation, 1);
        
        // Test stats update
        cache.update_stats_cache("Updated stats".to_string(), 2);
        assert_eq!(cache.get_cached_stats(), Some(&"Updated stats".to_string()));
        assert_eq!(cache.stats_generation, 2);
        
        // Test same generation (should not update)
        cache.update_stats_cache("Should not update".to_string(), 2);
        assert_eq!(cache.get_cached_stats(), Some(&"Updated stats".to_string()));
    }
    
    #[test]
    fn test_empty_filter_includes_all_items() {
        let mut cache = UiCache::default();
        let items = vec!["apple", "banana", "cherry"];
        
        cache.update_filter_cache("", &items, |item, filter| item.contains(filter));
        let indices = cache.get_filtered_indices();
        assert_eq!(indices.len(), 3);
        assert_eq!(indices, &[0, 1, 2]);
    }
}