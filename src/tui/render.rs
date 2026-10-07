// Draw the application data. Drawing never changes it.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap};

use crate::app::state::{App, Delivery, View};
use crate::tui::theme;

pub fn draw(frame: &mut Frame, app: &App) {
    let screen = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BACKGROUND).fg(theme::TEXT)),
        screen,
    );
    if screen.width < 40 || screen.height < 10 {
        frame.render_widget(Paragraph::new("Resize to 40 × 10"), screen);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .margin(1)
    .split(screen);
    let tabs = Tabs::new(["chat", "files"])
        .select(usize::from(app.view == View::Files))
        .padding("", "  ")
        .divider(" ")
        .style(Style::default().fg(theme::MUTED))
        .highlight_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, layout[0]);
    match app.view {
        View::Messages => draw_messages(frame, layout[1], app),
        View::Files => frame.render_widget(
            Paragraph::new("No files yet").style(Style::default().fg(theme::MUTED)),
            layout[1],
        ),
    }

    let controls = if app.typing {
        "Enter send · Esc back"
    } else if app.view == View::Messages {
        "jk move · i write · Tab pages · q quit"
    } else {
        "Tab pages · q quit"
    };
    // Connection failures remain visible without a permanent status banner.
    let status = match app.network_status.as_str() {
        "Relay connected" => "",
        "Connecting to relay" => "connecting…",
        "Relay unavailable · retrying" => "relay unavailable · retrying",
        error => error,
    };
    frame.render_widget(
        Paragraph::new(vec![Line::raw(status), Line::raw(controls)])
            .style(Style::default().fg(theme::MUTED)),
        layout[2],
    );
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::horizontal([
        Constraint::Length(16),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(area);
    let people = List::new(
        std::iter::once(ListItem::new("global")).chain(
            app.people
                .iter()
                .map(|person| ListItem::new(person.name.as_str())),
        ),
    )
    .highlight_symbol("› ")
    .highlight_style(Style::default().fg(theme::ACCENT).bg(theme::SELECTION))
    .block(
        Block::default()
            .title("chats")
            .title_style(Style::default().fg(theme::MUTED))
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme::BORDER)),
    );
    let mut selection = ListState::default().with_selected(Some(app.selected_chat));
    frame.render_stateful_widget(people, columns[0], &mut selection);
    let (name, history, draft) = if app.selected_chat == 0 {
        ("global", &app.global_messages, &app.global_draft)
    } else if let Some(person) = app.people.get(app.selected_chat - 1) {
        (person.name.as_str(), &person.messages, &person.draft)
    } else {
        frame.render_widget(
            Paragraph::new("No one online").style(Style::default().fg(theme::MUTED)),
            columns[2],
        );
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(columns[2]);
    frame.render_widget(
        Paragraph::new(name).style(Style::default().add_modifier(Modifier::BOLD)),
        rows[0],
    );
    let mut lines = Vec::new();
    for message in history {
        let mut author = vec![Span::styled(
            if message.from_me {
                "you"
            } else {
                message.author.as_deref().unwrap_or(name)
            },
            Style::default().fg(if message.from_me {
                theme::ACCENT
            } else {
                theme::MUTED
            }),
        )];
        if let Some(route) = message.route {
            author.push(Span::styled(
                format!(" [{route}]"),
                Style::default().fg(theme::MUTED),
            ));
        }
        match message.delivery {
            Some(Delivery::Sending) => author.push(Span::raw(" …")),
            Some(Delivery::Unconfirmed) => author.push(Span::raw(" ?")),
            _ => {}
        }
        lines.extend([Line::from(author), Line::raw(&message.text), Line::raw("")]);
    }
    lines.pop();
    let messages = Paragraph::new(lines).wrap(Wrap { trim: false });
    let scroll = messages
        .line_count(rows[1].width)
        .saturating_sub(usize::from(rows[1].height));
    frame.render_widget(
        messages.scroll((scroll.min(usize::from(u16::MAX)) as u16, 0)),
        rows[1],
    );

    let input = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(if app.typing {
            theme::ACCENT
        } else {
            theme::BORDER
        }));
    let inside = input.inner(rows[2]);
    frame.render_widget(input, rows[2]);
    let input_columns =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).split(inside);
    frame.render_widget(
        Paragraph::new("> ").style(Style::default().fg(if app.typing {
            theme::ACCENT
        } else {
            theme::MUTED
        })),
        input_columns[0],
    );
    if draft.is_empty() && !app.typing {
        frame.render_widget(
            Paragraph::new(if app.people.is_empty() {
                "No one online"
            } else {
                "i to write"
            })
            .style(Style::default().fg(theme::MUTED)),
            input_columns[1],
        );
        return;
    }
    let text = Line::raw(draft);
    let text_width = u16::try_from(text.width()).unwrap_or(u16::MAX);
    // Leave room for the cursor and keep the end of long drafts visible.
    let scroll = text_width.saturating_sub(input_columns[1].width.saturating_sub(1));
    frame.render_widget(Paragraph::new(text).scroll((0, scroll)), input_columns[1]);
    if app.typing {
        frame.set_cursor_position((input_columns[1].x + text_width - scroll, inside.y));
    }
}
