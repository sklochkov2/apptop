use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

use crate::app::{App, SortColumn, format_mb};

fn header_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn sort_indicator(col: SortColumn, current: SortColumn, ascending: bool) -> &'static str {
    if col == current {
        if ascending { " ▲" } else { " ▼" }
    } else {
        ""
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(frame.area());

    draw_summary(frame, app, chunks[0]);
    draw_table(frame, app, chunks[1]);
    draw_help(frame, chunks[2]);
}

fn draw_summary(frame: &mut Frame, app: &App, area: Rect) {
    let text = vec![
        Line::from(vec![
            Span::styled(
                "apptop",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" — "),
            Span::styled(
                format!("{} apps", app.entries.len()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(", "),
            Span::styled(
                format!("{} procs", app.total_procs),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(", "),
            Span::styled(
                format!("{} threads", app.total_threads),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::raw("PSS: "),
            Span::styled(format_mb(app.total_pss), Style::default().fg(Color::Cyan)),
            Span::raw("  Swap: "),
            Span::styled(
                format_mb(app.total_swap),
                Style::default().fg(Color::Magenta),
            ),
            Span::raw("  Total: "),
            Span::styled(
                format_mb(app.total_mem),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_table(frame: &mut Frame, app: &App, area: Rect) {
    let sc = app.sort_col;
    let asc = app.sort_ascending;

    let header_cells =
        SortColumn::ALL.map(|col| format!("{}{}", col.label(), sort_indicator(col, sc, asc)));

    let header = Row::new(header_cells).style(header_style()).height(1);

    let visible_rows = (area.height as usize).saturating_sub(3);
    let end = (app.scroll_offset + visible_rows).min(app.entries.len());
    let visible = &app.entries[app.scroll_offset..end];

    let rows = visible.iter().map(|e| {
        Row::new(vec![
            e.user.clone(),
            format!("{}", e.num_procs),
            format!("{}", e.threads),
            format_mb(e.pss_kb),
            format_mb(e.swap_kb),
            format_mb(e.total_kb),
            format!("{}", e.oom_max),
            e.name.clone(),
        ])
    });

    let widths = [
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(5),
        Constraint::Fill(1),
    ];

    let title = format!(
        " Memory by Application [{}/{}] ",
        (app.scroll_offset + 1).min(app.entries.len()),
        app.entries.len()
    );

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .column_spacing(1);

    frame.render_widget(table, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(
            " q",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Quit  "),
        Span::styled(
            "s",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Sort  "),
        Span::styled(
            "r",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Reverse  "),
        Span::styled(
            "1-8",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Sort col  "),
        Span::styled(
            "↑↓",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Scroll  "),
        Span::styled(
            "PgUp/Dn",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Page  "),
        Span::styled(
            "Home/End",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Jump"),
    ]);
    frame.render_widget(Paragraph::new(help), area);
}
