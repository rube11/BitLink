// Draw the application data. Drawing never changes it.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Tabs, Wrap,
};

use crate::app::state::{App, View};
use crate::tui::theme;

pub fn draw(frame: &mut Frame, app: &App) {
    let screen = frame.area();
    let background = Block::default().style(Style::default().bg(theme::BACKGROUND).fg(theme::TEXT));
    frame.render_widget(background, screen);

    if screen.width < 64 || screen.height < 16 {
        let notice = Paragraph::new("Resize to at least 64 x 16. Ctrl+C quits.")
            .style(Style::default().fg(theme::MUTED))
            .wrap(Wrap { trim: false });
        frame.render_widget(notice, screen);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .margin(1)
    .split(screen);
    let header_area = layout[0];
    let tabs_area = layout[1];
    let content_area = layout[2];
    let controls_area = layout[3];

    draw_header(frame, header_area);
    draw_tabs(frame, tabs_area, &app.view);

    match app.view {
        View::Announcements => {
            draw_announcements(frame, content_area, app);
        }
        View::Messages => {
            draw_messages(frame, content_area, app);
        }
        View::Files => {
            draw_files(frame, content_area);
        }
    }

    draw_controls(frame, controls_area, app);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let columns = Layout::horizontal([Constraint::Min(1), Constraint::Length(14)]).split(area);
    let brand = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                " B ",
                Style::default().bg(theme::ACCENT).fg(theme::BACKGROUND),
            ),
            Span::styled(
                "  Bit to Byte",
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            "Your club, one terminal.",
            Style::default().fg(theme::MUTED),
        ),
    ]);
    frame.render_widget(brand, columns[0]);

    let mode = Paragraph::new(" CLUB CHAT ")
        .style(Style::default().fg(theme::GOLD))
        .alignment(Alignment::Right);
    frame.render_widget(mode, columns[1]);
}

fn draw_tabs(frame: &mut Frame, area: Rect, view: &View) {
    let selected_tab = match view {
        View::Announcements => 0,
        View::Messages => 1,
        View::Files => 2,
    };
    let tabs = Tabs::new(vec!["Announcements", "Messages", "Files"])
        .select(selected_tab)
        .padding("  ", "  ")
        .divider(" ")
        .style(Style::default().fg(theme::MUTED))
        .highlight_style(
            Style::default()
                .bg(theme::SELECTION)
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme::BORDER)),
        );
    frame.render_widget(tabs, area);
}

fn draw_announcements(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
    let heading = Paragraph::new("THE CLUB NOTICEBOARD").style(
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(heading, rows[0]);
    let notices_area = rows[1];

    // Use a compact list to fit the notices in a short terminal.
    let card_space = app.announcements.len() * 4;
    if usize::from(notices_area.height) + 1 < card_space {
        let mut lines = Vec::new();
        for announcement in &app.announcements {
            lines.push(Line::raw(announcement.as_str()));
            lines.push(Line::raw(""));
        }
        let notices = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(notices, notices_area);
        return;
    }

    let mut top = notices_area.y;
    let mut number = 1;
    for announcement in &app.announcements {
        if top >= notices_area.bottom() {
            break;
        }
        let height = std::cmp::min(3, notices_area.bottom() - top);
        let card_area = Rect::new(notices_area.x, top, notices_area.width, height);
        let notice = Paragraph::new(vec![
            Line::styled(
                format!("NOTICE {:02}", number),
                Style::default().fg(theme::MUTED),
            ),
            Line::raw(announcement.as_str()),
        ])
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(theme::ACCENT))
                .padding(Padding::new(2, 1, 0, 0))
                .style(Style::default().bg(theme::PANEL)),
        );
        frame.render_widget(notice, card_area);
        top += height + 1;
        number += 1;
    }
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::horizontal([
        Constraint::Length(22),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(area);
    let people_area = columns[0];
    let conversation_area = columns[2];
    draw_people(frame, people_area, app);

    let person = match app.people.get(app.selected_person) {
        Some(person) => person,
        None => {
            let empty = Paragraph::new(vec![
                Line::styled(
                    "No conversations yet",
                    Style::default()
                        .fg(theme::TEXT)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::raw("Open Bit to Byte on another device or in another terminal."),
            ])
            .style(Style::default().fg(theme::MUTED))
            .wrap(Wrap { trim: false });
            frame.render_widget(empty, conversation_area);
            return;
        }
    };

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(conversation_area);
    let heading_area = rows[0];
    let messages_area = rows[1];
    let input_area = rows[2];

    let description = "Relayed chat · unencrypted demo";
    let heading = Paragraph::new(vec![
        Line::styled(
            person.name.as_str(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::styled(description, Style::default().fg(theme::MUTED)),
    ]);
    frame.render_widget(heading, heading_area);

    let mut messages = Vec::new();
    for message in &person.messages {
        let mut author = person.name.as_str();
        let mut author_color = theme::MUTED;
        if message.from_me {
            author = "You";
            author_color = theme::ACCENT;
        }
        let body = match &message.delivery {
            Some(delivery) => format!("[{}] {}", delivery.label(), message.text),
            None => message.text.clone(),
        };
        let mut lines = vec![
            Line::styled(author, Style::default().fg(author_color)),
            Line::raw(body),
        ];
        if messages_area.height > 2 {
            lines.push(Line::raw(""));
        }
        messages.push(ListItem::new(lines));
    }
    let message_list = List::new(messages);

    // Selecting the last item keeps the newest message visible as before.
    let mut last_message = ListState::default();
    if !person.messages.is_empty() {
        last_message.select(Some(person.messages.len() - 1));
    }
    frame.render_stateful_widget(message_list, messages_area, &mut last_message);
    draw_input(frame, input_area, &person.draft, app.typing);
}

fn draw_people(frame: &mut Frame, area: Rect, app: &App) {
    let mut people = Vec::new();
    for person in &app.people {
        let mut status = "[offline]";
        let mut status_color = theme::MUTED;
        if person.online {
            status = "[online]";
            status_color = theme::ACCENT;
        }
        let person_label = vec![
            Line::styled(
                person.name.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::styled(status, Style::default().fg(status_color)),
        ];
        people.push(ListItem::new(person_label));
    }
    let people_list = List::new(people)
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(theme::ACCENT).bg(theme::SELECTION))
        .block(
            Block::default()
                .title(" PEOPLE ")
                .title_bottom(format!(" {} people ", app.people.len()))
                .title_style(Style::default().fg(theme::MUTED))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::BORDER))
                .padding(Padding::new(1, 1, 1, 0))
                .style(Style::default().bg(theme::PANEL)),
        );
    let mut selection = ListState::default();
    selection.select(Some(app.selected_person));
    frame.render_stateful_widget(people_list, area, &mut selection);
}

fn draw_input(frame: &mut Frame, area: Rect, draft: &str, typing: bool) {
    let mut title = " MESSAGE ";
    let mut color = theme::BORDER;
    let mut title_color = theme::MUTED;
    if typing {
        title = " WRITING ";
        color = theme::ACCENT;
        title_color = theme::ACCENT;
    }
    let border = Block::default()
        .title(title)
        .title_style(Style::default().fg(title_color))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .padding(Padding::new(1, 1, 0, 0))
        .style(Style::default().bg(theme::PANEL));
    let inside = border.inner(area);

    if draft.is_empty() && !typing {
        let placeholder = Paragraph::new("Enter to start a message")
            .style(Style::default().fg(theme::MUTED))
            .block(border);
        frame.render_widget(placeholder, area);
        return;
    }

    let text = Line::raw(draft);
    let text_width = match u16::try_from(text.width()) {
        Ok(width) => width,
        Err(_) => return,
    };

    // Leave one column for the cursor and keep the end of long drafts visible.
    let available_width = inside.width.saturating_sub(1);
    let scroll = text_width.saturating_sub(available_width);
    let input = Paragraph::new(text).block(border).scroll((0, scroll));
    frame.render_widget(input, area);

    if typing {
        let cursor_x = inside.x + text_width - scroll;
        frame.set_cursor_position((cursor_x, inside.y));
    }
}

fn draw_files(frame: &mut Frame, area: Rect) {
    let panel = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::PANEL));
    frame.render_widget(panel, area);

    let width = std::cmp::min(area.width - 2, 58);
    let height = std::cmp::min(area.height - 2, 6);
    let message_area = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let empty = Paragraph::new(vec![
        Line::styled("[ + ]", Style::default().fg(theme::ACCENT)),
        Line::raw(""),
        Line::styled(
            "A shared shelf for your club",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled(
            "File sharing will be added later.",
            Style::default().fg(theme::MUTED),
        ),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(empty, message_area);
}

fn draw_controls(frame: &mut Frame, area: Rect, app: &App) {
    let key_style = Style::default().fg(theme::ACCENT);
    let mut shortcuts = vec![
        Span::styled("Tab", key_style),
        Span::raw(" views   "),
        Span::styled("↑ ↓", key_style),
        Span::raw(" people   "),
        Span::styled("Enter", key_style),
        Span::raw(" write   "),
        Span::styled("q", key_style),
        Span::raw(" quit"),
    ];
    if app.typing {
        shortcuts = vec![
            Span::styled("Enter", key_style),
            Span::raw(" send   "),
            Span::styled("Esc", key_style),
            Span::raw(" keep draft   "),
            Span::styled("Ctrl+C", key_style),
            Span::raw(" quit"),
        ];
    }
    let controls = Paragraph::new(vec![
        Line::from(shortcuts),
        Line::raw(format!("{} · You: {}", app.network_status, app.name)),
    ])
    .style(Style::default().fg(theme::MUTED));
    frame.render_widget(controls, area);
}
