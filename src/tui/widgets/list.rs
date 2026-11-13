//! Generic selectable list widget with filtering, sorting, and keyboard navigation.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem as RatatuiListItem, ListState as RatatuiListState,
        Paragraph, Wrap,
    },
};

/// A trait for items that can be displayed in a ListWidget.
pub trait SelectableItem: Clone + Send + Sync {
    /// Unique identifier for the item.
    fn id(&self) -> String;

    /// Primary display text for the list row.
    fn title(&self) -> String;

    /// Optional secondary text (shown in detail or as subtitle).
    fn subtitle(&self) -> Option<String> {
        None
    }

    /// Optional icon/prefix for the item.
    fn prefix(&self) -> Option<&str> {
        None
    }

    /// Style override for this item (e.g., for warnings, errors).
    fn style(&self) -> Option<Style> {
        None
    }

    /// Searchable text for filtering (defaults to title + subtitle).
    fn search_text(&self) -> String {
        let mut text = self.title();
        if let Some(sub) = self.subtitle() {
            text.push(' ');
            text.push_str(&sub);
        }
        text
    }
}

/// State for a ListWidget including selection, scroll, and filter.
#[derive(Debug, Clone, Default)]
pub struct SelectableListState {
    pub list_state: RatatuiListState,
    pub filter: String,
    pub filtered_indices: Vec<usize>,
    pub sort_by: Option<SortKey>,
    pub sort_desc: bool,
}

impl SelectableListState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Select the next item (wraps).
    pub fn next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let selected = self.list_state.selected().unwrap_or(0);
        let next = (selected + 1).min(len - 1);
        self.list_state.select(Some(next));
    }

    /// Select the previous item (wraps).
    pub fn previous(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let selected = self.list_state.selected().unwrap_or(0);
        let prev = selected.saturating_sub(1);
        self.list_state.select(Some(prev));
    }

    /// Select first item.
    pub fn first(&mut self) {
        self.list_state.select(Some(0));
    }

    /// Select last item.
    pub fn last(&mut self, len: usize) {
        if len > 0 {
            self.list_state.select(Some(len - 1));
        }
    }

    /// Scroll by page (half screen).
    pub fn page_down(&mut self, len: usize, page_size: usize) {
        if len == 0 {
            return;
        }
        let selected = self.list_state.selected().unwrap_or(0);
        let next = (selected + page_size).min(len - 1);
        self.list_state.select(Some(next));
    }

    /// Scroll up by page.
    pub fn page_up(&mut self, page_size: usize) {
        let selected = self.list_state.selected().unwrap_or(0);
        let prev = selected.saturating_sub(page_size);
        self.list_state.select(Some(prev));
    }

    /// Get currently selected item index (in original list).
    pub fn selected_original(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i).copied())
    }

    /// Set filter text and rebuild filtered indices.
    pub fn set_filter<T: SelectableItem>(&mut self, filter: String, items: &[T]) {
        self.filter = filter.to_lowercase();
        self.rebuild_filtered(items);
        self.list_state.select(Some(0));
    }

    /// Rebuild filtered indices based on current filter.
    pub fn rebuild_filtered<T: SelectableItem>(&mut self, items: &[T]) {
        self.filtered_indices.clear();
        if self.filter.is_empty() {
            self.filtered_indices = (0..items.len()).collect();
        } else {
            for (idx, item) in items.iter().enumerate() {
                if item.search_text().to_lowercase().contains(&self.filter) {
                    self.filtered_indices.push(idx);
                }
            }
        }
        // Apply sort if needed
        if let Some(sort) = self.sort_by {
            self.filtered_indices.sort_by(|a, b| {
                let item_a = &items[*a];
                let item_b = &items[*b];
                let cmp = sort.compare(item_a, item_b);
                if self.sort_desc { cmp.reverse() } else { cmp }
            });
        }
    }

    /// Toggle sort by key.
    pub fn toggle_sort<T: SelectableItem>(&mut self, key: SortKey, items: &[T]) {
        if self.sort_by == Some(key) {
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_by = Some(key);
            self.sort_desc = false;
        }
        self.rebuild_filtered(items);
    }

    /// Cycle the sort key: title, subtitle, id, and then no sorting.
    pub fn cycle_sort<T: SelectableItem>(&mut self, items: &[T]) {
        let next = match self.sort_by {
            Some(SortKey::Title) => Some(SortKey::Subtitle),
            Some(SortKey::Subtitle) => Some(SortKey::Id),
            Some(SortKey::Id) => None,
            None => Some(SortKey::Title),
        };

        match next {
            Some(key) => self.toggle_sort(key, items),
            None => {
                self.sort_by = None;
                self.sort_desc = false;
                self.rebuild_filtered(items);
            }
        }
    }

    /// Reverse the sort order.
    pub fn reverse_sort<T: SelectableItem>(&mut self, items: &[T]) {
        self.sort_desc = !self.sort_desc;
        self.rebuild_filtered(items);
    }

    /// Clear filter.
    pub fn clear_filter<T: SelectableItem>(&mut self, items: &[T]) {
        self.filter.clear();
        self.rebuild_filtered(items);
        self.list_state.select(Some(0));
    }

    /// Get filtered items.
    pub fn filtered_items<'a, T: SelectableItem>(&self, items: &'a [T]) -> Vec<&'a T> {
        self.filtered_indices.iter().map(|&i| &items[i]).collect()
    }

    /// Get selected item.
    pub fn selected_item<'a, T: SelectableItem>(&self, items: &'a [T]) -> Option<&'a T> {
        self.selected_original().map(|i| &items[i])
    }
}

/// Sort keys for list items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Title,
    Subtitle,
    Id,
}

impl SortKey {
    fn compare<T: SelectableItem>(&self, a: &T, b: &T) -> std::cmp::Ordering {
        match self {
            SortKey::Title => a.title().cmp(&b.title()),
            SortKey::Subtitle => a
                .subtitle()
                .unwrap_or_default()
                .cmp(&b.subtitle().unwrap_or_default()),
            SortKey::Id => a.id().cmp(&b.id()),
        }
    }
}

/// A generic list widget with title, filter, and selection.
pub struct ListWidget<T: SelectableItem> {
    pub title: String,
    pub items: Vec<T>,
    pub state: SelectableListState,
    pub show_filter: bool,
    pub show_help: bool,
    pub empty_message: String,
}

impl<T: SelectableItem> ListWidget<T> {
    pub fn new(title: impl Into<String>, items: Vec<T>) -> Self {
        let mut state = SelectableListState::new();
        state.rebuild_filtered(&items);
        if !items.is_empty() {
            state.list_state.select(Some(0));
        }
        Self {
            title: title.into(),
            items,
            state,
            show_filter: false,
            show_help: false,
            empty_message: "No items found".to_string(),
        }
    }

    pub fn with_empty_message(mut self, msg: impl Into<String>) -> Self {
        self.empty_message = msg.into();
        self
    }

    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.state.rebuild_filtered(&self.items);
        if !self.items.is_empty() {
            self.state.list_state.select(Some(0));
        }
    }

    /// Replace the items, keeping the current selection.
    ///
    /// Tabs rebuild their list from the shared state on every frame, so the
    /// selection has to survive a refresh; otherwise the cursor would jump
    /// back to the first row on every render.
    pub fn set_items_preserving_selection(&mut self, items: Vec<T>) {
        let selected_id = self.selected().map(|item| item.id());
        self.set_items(items);

        let Some(id) = selected_id else {
            return;
        };
        let position = self
            .state
            .filtered_indices
            .iter()
            .position(|&original| self.items[original].id() == id);
        if let Some(position) = position {
            self.state.list_state.select(Some(position));
        }
    }

    pub fn selected(&self) -> Option<&T> {
        self.state.selected_item(&self.items)
    }

    /// The index of the selected item in the unfiltered list.
    ///
    /// Only used by the tests.
    #[cfg(test)]
    pub fn selected_index(&self) -> Option<usize> {
        self.state.selected_original()
    }

    pub fn filtered_count(&self) -> usize {
        self.state.filtered_indices.len()
    }

    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Handle keyboard input. Returns true if handled.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        let filtered_len = self.state.filtered_indices.len();

        match key.code {
            // Control shortcuts keep working while the filter is open.
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.first();
                true
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.page_down(filtered_len, 10);
                true
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.page_up(10);
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.show_filter {
                    self.state.clear_filter(&self.items);
                    true
                } else {
                    false
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Cycle sort: Title -> Subtitle -> Id -> None
                self.state.cycle_sort(&self.items);
                true
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.state.reverse_sort(&self.items);
                true
            }

            // While filtering, every printable character is part of the query.
            KeyCode::Char(c) if self.show_filter => {
                self.state
                    .set_filter(format!("{}{}", self.state.filter, c), &self.items);
                true
            }
            KeyCode::Backspace if self.show_filter => {
                let mut filter = self.state.filter.clone();
                filter.pop();
                self.state.set_filter(filter, &self.items);
                true
            }

            KeyCode::Char('j') => {
                self.state.next(filtered_len);
                true
            }
            KeyCode::Char('k') => {
                self.state.previous(filtered_len);
                true
            }
            KeyCode::Down if !self.show_filter => {
                self.state.next(filtered_len);
                true
            }
            KeyCode::Up if !self.show_filter => {
                self.state.previous(filtered_len);
                true
            }
            KeyCode::Char('g') => {
                self.state.first();
                true
            }
            KeyCode::Char('G') => {
                self.state.last(filtered_len);
                true
            }
            KeyCode::Char('/') => {
                self.show_filter = true;
                true
            }
            KeyCode::Esc => {
                if self.show_filter {
                    self.show_filter = false;
                    true
                } else if !self.state.filter.is_empty() {
                    self.state.clear_filter(&self.items);
                    true
                } else {
                    false
                }
            }
            KeyCode::Enter => true, // Let parent handle selection
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                true
            }
            _ => false,
        }
    }

    /// Render the list widget.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                // The title and the filter input are bordered blocks, so they
                // each need a row for their content plus two border rows.
                Constraint::Length(3),
                Constraint::Length(if self.show_filter { 3 } else { 0 }),
                Constraint::Min(0),
                Constraint::Length(if self.show_help { 3 } else { 0 }),
            ])
            .split(area);

        // Title + filter bar
        let title_area = chunks[0];
        let filter_area = chunks[1];
        let list_area = chunks[2];
        let help_area = chunks.get(3).copied().unwrap_or_default();

        // Build title line
        let mut title_spans = vec![
            Span::styled(
                self.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                " ({}/{})",
                self.filtered_count(),
                self.total_count()
            )),
        ];

        if let Some(sort) = self.state.sort_by {
            let sort_name = match sort {
                SortKey::Title => "Title",
                SortKey::Subtitle => "Subtitle",
                SortKey::Id => "ID",
            };
            let dir = if self.state.sort_desc { " ↓" } else { " ↑" };
            title_spans.push(Span::styled(
                format!(" [{}{}]", sort_name, dir),
                Style::default().fg(Color::Yellow),
            ));
        }

        if !self.state.filter.is_empty() {
            title_spans.push(Span::styled(
                format!(" 🔍 {}", self.state.filter),
                Style::default().fg(Color::Cyan),
            ));
        }

        let title =
            Paragraph::new(Line::from(title_spans)).block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, title_area);

        // Filter input
        if self.show_filter {
            let filter_text = Paragraph::new(format!("/{}", self.state.filter))
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("Filter"));
            frame.render_widget(filter_text, filter_area);
        }

        // List items
        let filtered = self.state.filtered_items(&self.items);
        if filtered.is_empty() {
            let empty = Paragraph::new(self.empty_message.clone())
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL))
                .wrap(Wrap { trim: true });
            frame.render_widget(empty, list_area);
        } else {
            let items: Vec<RatatuiListItem> = filtered
                .iter()
                .map(|item| {
                    let mut spans = Vec::new();
                    if let Some(prefix) = item.prefix() {
                        spans.push(Span::styled(prefix, Style::default().fg(Color::Blue)));
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::raw(item.title()));
                    if let Some(sub) = item.subtitle() {
                        spans.push(Span::raw("  "));
                        spans.push(Span::styled(sub, Style::default().fg(Color::DarkGray)));
                    }
                    let mut li = RatatuiListItem::new(Line::from(spans));
                    if let Some(style) = item.style() {
                        li = li.style(style);
                    }
                    li
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("► ");

            frame.render_stateful_widget(list, list_area, &mut self.state.list_state);
        }

        // Help
        if self.show_help {
            let help_text = "j/k ↑/down: navigate | g: top | G: bottom | Ctrl+d/u: page | /: filter | Esc: clear filter | Enter: select | Ctrl+s: sort | Ctrl+r: reverse | ?: help";
            let help = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title("Help"))
                .wrap(Wrap { trim: true });
            frame.render_widget(help, help_area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Item {
        id: String,
        title: String,
        subtitle: Option<String>,
    }

    impl SelectableItem for Item {
        fn id(&self) -> String {
            self.id.clone()
        }

        fn title(&self) -> String {
            self.title.clone()
        }

        fn subtitle(&self) -> Option<String> {
            self.subtitle.clone()
        }
    }

    fn item(id: &str, title: &str) -> Item {
        Item {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: Some(format!("{id}-note")),
        }
    }

    fn items() -> Vec<Item> {
        vec![item("a", "alpha"), item("b", "bravo"), item("c", "charlie")]
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn render(list: &mut ListWidget<Item>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| list.render(frame, frame.area()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn starts_with_the_first_item_selected() {
        let list = ListWidget::new("Items", items());
        assert_eq!(list.filtered_count(), 3);
        assert_eq!(list.total_count(), 3);
        assert_eq!(
            list.selected().map(|item| item.id.clone()),
            Some("a".to_string())
        );
    }

    #[test]
    fn navigation_moves_the_selection() {
        let mut list = ListWidget::new("Items", items());

        assert!(list.handle_key(key(KeyCode::Char('j'))));
        assert_eq!(list.selected_index(), Some(1));

        assert!(list.handle_key(key(KeyCode::Char('k'))));
        assert_eq!(list.selected_index(), Some(0));

        assert!(list.handle_key(key(KeyCode::Char('G'))));
        assert_eq!(list.selected_index(), Some(2));

        assert!(list.handle_key(key(KeyCode::Char('g'))));
        assert_eq!(list.selected_index(), Some(0));
    }

    #[test]
    fn filter_narrows_the_visible_items() {
        let mut list = ListWidget::new("Items", items());

        assert!(list.handle_key(key(KeyCode::Char('/'))));
        assert!(list.show_filter);

        assert!(list.handle_key(key(KeyCode::Char('b'))));
        assert_eq!(list.filtered_count(), 1);
        assert_eq!(
            list.selected().map(|item| item.id.clone()),
            Some("b".to_string())
        );
        assert_eq!(list.total_count(), 3);
    }

    #[test]
    fn escape_clears_an_active_filter() {
        let mut list = ListWidget::new("Items", items());
        list.state.set_filter("bra".to_string(), &list.items);
        assert_eq!(list.filtered_count(), 1);

        assert!(list.handle_key(key(KeyCode::Esc)));
        assert_eq!(list.filtered_count(), 3);
        assert!(list.state.filter.is_empty());
    }

    #[test]
    fn sort_orders_items_by_title() {
        let mut list = ListWidget::new(
            "Items",
            vec![item("c", "charlie"), item("a", "alpha"), item("b", "bravo")],
        );

        list.state.toggle_sort(SortKey::Title, &list.items);

        let titles: Vec<String> = list
            .state
            .filtered_items(&list.items)
            .iter()
            .map(|item| item.title.clone())
            .collect();
        assert_eq!(titles, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn sorting_twice_reverses_the_order() {
        let mut list = ListWidget::new("Items", items());

        list.state.toggle_sort(SortKey::Title, &list.items);
        list.state.toggle_sort(SortKey::Title, &list.items);

        let titles: Vec<String> = list
            .state
            .filtered_items(&list.items)
            .iter()
            .map(|item| item.title.clone())
            .collect();
        assert_eq!(titles, vec!["charlie", "bravo", "alpha"]);
    }

    #[test]
    fn set_items_resets_the_selection() {
        let mut list = ListWidget::new("Items", items());
        list.handle_key(key(KeyCode::Char('j')));
        assert_eq!(list.selected_index(), Some(1));

        list.set_items(vec![item("z", "zulu")]);

        assert_eq!(list.selected_index(), Some(0));
        assert_eq!(
            list.selected().map(|item| item.id.clone()),
            Some("z".to_string())
        );
    }

    #[test]
    fn set_items_preserving_selection_keeps_the_cursor() {
        let mut list = ListWidget::new("Items", items());
        list.handle_key(key(KeyCode::Char('j')));
        assert_eq!(list.selected_index(), Some(1));

        list.set_items_preserving_selection(items());

        assert_eq!(list.selected_index(), Some(1));
        assert_eq!(
            list.selected().map(|item| item.id.clone()),
            Some("b".to_string())
        );
    }

    #[test]
    fn set_items_preserving_selection_falls_back_when_the_item_is_gone() {
        let mut list = ListWidget::new("Items", items());
        list.handle_key(key(KeyCode::Char('j')));

        list.set_items_preserving_selection(vec![item("z", "zulu")]);

        assert_eq!(list.selected_index(), Some(0));
        assert_eq!(
            list.selected().map(|item| item.id.clone()),
            Some("z".to_string())
        );
    }

    #[test]
    fn renders_item_titles_and_counts() {
        let mut list = ListWidget::new("Items", items());
        let text = render(&mut list, 40, 10);

        assert!(text.contains("Items"));
        assert!(text.contains("(3/3)"));
        assert!(text.contains("alpha"));
        assert!(text.contains("charlie"));
    }

    #[test]
    fn renders_the_empty_message_when_there_are_no_items() {
        let mut list = ListWidget::new("Items", Vec::new()).with_empty_message("Nothing here yet.");
        let text = render(&mut list, 40, 10);

        assert!(text.contains("Nothing here yet."));
    }
}
