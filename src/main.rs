use std::{
    error::Error,
    io::{self, stdout},
    time::Duration,
};

use chrono::NaiveDate;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Cell, Chart, Dataset, GraphType, List, ListItem, Paragraph, Row,
        Table, TableState,
    },
};

const DATE_FMT: &str = "%Y-%m-%d";

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new();
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("error: {err}");
    }
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    mut app: App,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(200);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if !event::poll(tick_rate)? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => match app.mode {
                Mode::Portfolio => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('a') => {
                        app.mode = Mode::AddForm;
                        app.form = AddForm::new();
                    }
                    KeyCode::Char('d') | KeyCode::Enter => {
                        if !app.positions.is_empty() {
                            app.mode = Mode::Detail;
                        }
                    }
                    KeyCode::Char('h') => app.mode = Mode::Help,
                    KeyCode::Down => app.select_next(),
                    KeyCode::Up => app.select_prev(),
                    _ => {}
                },
                Mode::Detail => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Portfolio,
                    KeyCode::Char('q') => break,
                    KeyCode::Down => app.select_next(),
                    KeyCode::Up => app.select_prev(),
                    KeyCode::Char('a') => {
                        app.mode = Mode::AddForm;
                        app.form = AddForm::new();
                    }
                    _ => {}
                },
                Mode::Help => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') | KeyCode::Enter => {
                        app.mode = Mode::Portfolio
                    }
                    KeyCode::Char('q') => break,
                    _ => {}
                },
                Mode::AddForm => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Portfolio;
                    }
                    KeyCode::Enter => {
                        if app.form.on_enter() {
                            match app.form.try_build_position() {
                                Ok(pos) => {
                                    app.positions.push(pos);
                                    app.selected = app.positions.len().saturating_sub(1);
                                    app.mode = Mode::Portfolio;
                                }
                                Err(msg) => app.form.error = Some(msg),
                            }
                        } else {
                            app.form.next_field();
                        }
                    }
                    KeyCode::Tab => app.form.next_field(),
                    KeyCode::BackTab => app.form.prev_field(),
                    KeyCode::Backspace => app.form.backspace(),
                    KeyCode::Left => app.form.backspace(),
                    KeyCode::Right => app.form.next_field(),
                    KeyCode::Char(c) => {
                        if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                            app.form.push_char(c);
                        }
                    }
                    _ => {}
                },
            },
            Event::Resize(_, _) => {} // redraw happens next loop
            _ => {}
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct Position {
    cost_per_share: f64,
    quantity: f64,
    sale_price: f64,
    purchase_date: NaiveDate,
    sale_date: NaiveDate,
}

impl Position {
    fn invested(&self) -> f64 {
        self.cost_per_share * self.quantity
    }

    fn proceeds(&self) -> f64 {
        self.sale_price * self.quantity
    }

    fn roi_value(&self) -> f64 {
        self.proceeds() - self.invested()
    }

    fn roi_pct(&self) -> f64 {
        self.roi_value() / self.invested()
    }

    fn days_held(&self) -> i64 {
        let days = (self.sale_date - self.purchase_date).num_days();
        days.max(1)
    }

    fn roi_per_day(&self) -> f64 {
        self.roi_pct() / (self.days_held() as f64)
    }

    fn annualized_roi(&self) -> f64 {
        let multiple = self.proceeds() / self.invested();
        if multiple <= 0.0 {
            return -1.0;
        }
        let years = self.days_held() as f64 / 365.0;
        multiple.powf(1.0 / years) - 1.0
    }
}

#[derive(Clone)]
struct Field {
    label: &'static str,
    placeholder: &'static str,
    value: String,
}

impl Field {
    fn new(label: &'static str, placeholder: &'static str) -> Self {
        Self {
            label,
            placeholder,
            value: String::new(),
        }
    }
}

#[derive(Clone)]
struct AddForm {
    fields: Vec<Field>,
    active: usize,
    error: Option<String>,
}

impl Default for AddForm {
    fn default() -> Self {
        Self::new()
    }
}

impl AddForm {
    fn new() -> Self {
        Self {
            fields: vec![
                Field::new("Cost/share", "e.g. 112.40"),
                Field::new("Quantity", "e.g. 50"),
                Field::new("Sale price", "e.g. 128.70"),
                Field::new("Purchase date", "YYYY-MM-DD"),
                Field::new("Sale date", "YYYY-MM-DD"),
            ],
            active: 0,
            error: None,
        }
    }

    fn on_enter(&self) -> bool {
        self.active >= self.fields.len().saturating_sub(1)
    }

    fn next_field(&mut self) {
        self.active = (self.active + 1) % self.fields.len();
    }

    fn prev_field(&mut self) {
        if self.active == 0 {
            self.active = self.fields.len() - 1;
        } else {
            self.active -= 1;
        }
    }

    fn backspace(&mut self) {
        if let Some(ch) = self.fields.get_mut(self.active) {
            ch.value.pop();
        }
    }

    fn push_char(&mut self, c: char) {
        if let Some(ch) = self.fields.get_mut(self.active) {
            ch.value.push(c);
        }
    }

    fn try_build_position(&self) -> Result<Position, String> {
        let cost = parse_f64(&self.fields[0].value, "cost/share")?;
        let qty = parse_f64(&self.fields[1].value, "quantity")?;
        let sale_price = parse_f64(&self.fields[2].value, "sale price")?;
        let purchase_date = parse_date(&self.fields[3].value, "purchase date")?;
        let sale_date = parse_date(&self.fields[4].value, "sale date")?;

        if sale_date < purchase_date {
            return Err("Sale date cannot be before purchase date".into());
        }

        Ok(Position {
            cost_per_share: cost,
            quantity: qty,
            sale_price,
            purchase_date,
            sale_date,
        })
    }
}

fn parse_f64(raw: &str, label: &str) -> Result<f64, String> {
    raw.trim()
        .parse::<f64>()
        .map_err(|_| format!("Invalid {label}"))
}

fn parse_date(raw: &str, label: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw.trim(), DATE_FMT)
        .map_err(|_| format!("Invalid {label}, expected YYYY-MM-DD"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Portfolio,
    Detail,
    AddForm,
    Help,
}

struct App {
    positions: Vec<Position>,
    selected: usize,
    mode: Mode,
    form: AddForm,
}

impl App {
    fn new() -> Self {
        let today = chrono::Utc::now().date_naive();
        Self {
            positions: vec![
                Position {
                    cost_per_share: 110.0,
                    quantity: 40.0,
                    sale_price: 127.5,
                    purchase_date: today - chrono::Days::new(12),
                    sale_date: today,
                },
                Position {
                    cost_per_share: 64.0,
                    quantity: 100.0,
                    sale_price: 59.4,
                    purchase_date: today - chrono::Days::new(4),
                    sale_date: today,
                },
                Position {
                    cost_per_share: 320.5,
                    quantity: 10.0,
                    sale_price: 355.2,
                    purchase_date: today - chrono::Days::new(25),
                    sale_date: today - chrono::Days::new(5),
                },
            ],
            selected: 0,
            mode: Mode::Portfolio,
            form: AddForm::new(),
        }
    }

    fn select_next(&mut self) {
        if self.positions.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % self.positions.len();
    }

    fn select_prev(&mut self) {
        if self.positions.is_empty() {
            self.selected = 0;
            return;
        }
        if self.selected == 0 {
            self.selected = self.positions.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn selected_position(&self) -> Option<&Position> {
        self.positions.get(self.selected)
    }
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.size();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    draw_header(f, vertical[0], app);

    match app.mode {
        Mode::Portfolio => draw_portfolio(f, vertical[1], app),
        Mode::Detail => draw_detail(f, vertical[1], app),
        Mode::AddForm => draw_form(f, size, app),
        Mode::Help => draw_help(f, size),
    }

    draw_footer(f, vertical[2], app.mode);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let (total_invested, total_proceeds, roi_pct) = portfolio_stats(&app.positions);
    let title = Line::from(vec![
        Span::styled(
            " ROI Tracker ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  invested "),
        Span::styled(
            format_currency(total_invested),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  proceeds "),
        Span::styled(
            format_currency(total_proceeds),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  ROI "),
        styled_roi_pct(roi_pct),
    ]);

    let block = Paragraph::new(title).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Portfolio Snapshot"),
    );
    f.render_widget(block, area);
}

fn draw_footer(f: &mut Frame, area: Rect, mode: Mode) {
    let hint = match mode {
        Mode::Portfolio => "↑/↓ select  • enter/d detail  • a add  • h help  • q quit",
        Mode::Detail => "↑/↓ move  • b/esc back  • a add  • q quit",
        Mode::AddForm => "tab/shift+tab move  • enter next/save  • esc cancel",
        Mode::Help => "enter/esc back  • q quit",
    };
    let footer = Paragraph::new(Line::from(hint))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, area);
}

fn draw_portfolio(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);

    draw_positions_table(f, chunks[0], app);
    draw_portfolio_chart(f, chunks[1], app);
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_positions_table(f, chunks[0], app);
    draw_position_detail(f, chunks[1], app);
}

fn draw_positions_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        "Pos", "Cost", "Qty", "Sale", "ROI%", "Ann%", "Days", "Bought", "Sold",
    ])
    .style(Style::default().fg(Color::Yellow));

    let rows: Vec<Row> = app
        .positions
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let roi = Cell::from(styled_roi_pct(p.roi_pct()));
            let ann = Cell::from(styled_roi_pct(p.annualized_roi()));
            Row::new(vec![
                Cell::from(format!("#{idx}")),
                Cell::from(format_currency(p.cost_per_share)),
                Cell::from(format!("{:.2}", p.quantity)),
                Cell::from(format_currency(p.sale_price)),
                roi,
                ann,
                Cell::from(p.days_held().to_string()),
                Cell::from(p.purchase_date.format(DATE_FMT).to_string()),
                Cell::from(p.sale_date.format(DATE_FMT).to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Positions"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = TableState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_portfolio_chart(f: &mut Frame, area: Rect, app: &App) {
    let points: Vec<(f64, f64)> = app
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| (i as f64, p.roi_pct() * 100.0))
        .collect();

    let y_bounds = bounds_from_points(&points, -5.0, 5.0);
    let x_bounds = if points.is_empty() {
        [0.0, 1.0]
    } else {
        [0.0, (points.len() - 1) as f64]
    };

    let dataset = Dataset::default()
        .name("ROI % by position")
        .marker(symbols::Marker::Dot)
        .style(Style::default().fg(Color::Cyan))
        .data(&points);

    let chart = Chart::new(vec![dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Portfolio ROI graph"),
        )
        .x_axis(Axis::default().bounds(x_bounds).labels({
            let last = if points.is_empty() {
                "1".to_string()
            } else {
                (points.len() - 1).to_string()
            };
            vec![Span::raw("0"), Span::raw(last)]
        }))
        .y_axis(
            Axis::default()
                .title("ROI %")
                .style(Style::default().fg(Color::Gray))
                .bounds(y_bounds)
                .labels(vec![
                    Span::raw(format!("{:.0}", y_bounds[0])),
                    Span::raw("0"),
                    Span::raw(format!("{:.0}", y_bounds[1])),
                ]),
        );

    f.render_widget(chart, area);
}

fn draw_position_detail(f: &mut Frame, area: Rect, app: &App) {
    let Some(pos) = app.selected_position() else {
        let block =
            Paragraph::new("No position selected").block(Block::default().borders(Borders::ALL));
        f.render_widget(block, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(5)])
        .split(area);

    let info = vec![
        Line::from(vec![
            Span::styled("ROI ", Style::default().fg(Color::Gray)),
            styled_roi_pct(pos.roi_pct()),
            Span::raw("  "),
            Span::styled("Annualized ", Style::default().fg(Color::Gray)),
            styled_roi_pct(pos.annualized_roi()),
        ]),
        Line::from(vec![
            Span::styled("ROI/day ", Style::default().fg(Color::Gray)),
            styled_roi_pct(pos.roi_per_day()),
        ]),
        Line::from(vec![
            Span::styled("PnL ", Style::default().fg(Color::Gray)),
            Span::styled(
                format_currency(pos.roi_value()),
                Style::default().fg(if pos.roi_value() >= 0.0 {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
        ]),
        Line::from(format!(
            "Held {} days  {} -> {}",
            pos.days_held(),
            pos.purchase_date.format(DATE_FMT),
            pos.sale_date.format(DATE_FMT)
        )),
        Line::from(format!(
            "Invested {}  Proceeds {}  Qty {:.2}",
            format_currency(pos.invested()),
            format_currency(pos.proceeds()),
            pos.quantity
        )),
    ];

    let info_block = Paragraph::new(info).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Position detail"),
    );
    f.render_widget(info_block, chunks[0]);

    let duration = pos.days_held().max(1) as f64;
    let points = vec![(0.0, 0.0), (duration, pos.roi_pct() * 100.0)];
    let y_bounds = bounds_from_points(&points, -5.0, 5.0);
    let x_bounds = [0.0, duration.max(1.0)];

    let dataset = Dataset::default()
        .name("ROI over hold")
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Magenta))
        .data(&points);

    let chart = Chart::new(vec![dataset])
        .block(Block::default().borders(Borders::ALL).title("ROI timeline"))
        .x_axis(
            Axis::default()
                .title("Days held")
                .bounds(x_bounds)
                .labels(vec![Span::raw("0"), Span::raw(format!("{:.0}", duration))]),
        )
        .y_axis(Axis::default().title("ROI %").bounds(y_bounds).labels(vec![
            Span::raw(format!("{:.0}", y_bounds[0])),
            Span::raw("0"),
            Span::raw(format!("{:.0}", y_bounds[1])),
        ]));

    f.render_widget(chart, chunks[1]);
}

fn draw_form(f: &mut Frame, area: Rect, app: &App) {
    let form_area = centered_rect(70, 70, area);
    let block = Block::default()
        .title("Add position")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block, form_area);

    let inner = form_area.inner(&ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });

    let mut items = Vec::new();
    for (idx, field) in app.form.fields.iter().enumerate() {
        let active = idx == app.form.active;
        let label = if active {
            Span::styled(
                field.label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(field.label, Style::default().fg(Color::Gray))
        };

        let value = if field.value.is_empty() {
            Span::styled(field.placeholder, Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(field.value.as_str())
        };

        items.push(ListItem::new(Line::from(vec![
            label,
            Span::raw(": "),
            value,
        ])));
    }

    if let Some(err) = &app.form.error {
        items.push(ListItem::new(Span::styled(
            err,
            Style::default().fg(Color::Red),
        )));
    }

    let list = List::new(items).block(Block::default());
    f.render_widget(list, inner);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("ROI Tracker TUI"),
        Line::from(" "),
        Line::from("Portfolio view:"),
        Line::from("  - ↑/↓ move selection"),
        Line::from("  - enter/d open position detail"),
        Line::from("  - a add a new position"),
        Line::from("  - h open this help, q quit"),
        Line::from(" "),
        Line::from("Form view:"),
        Line::from("  - tab / shift+tab to move"),
        Line::from("  - enter to advance or save on last field"),
        Line::from("  - esc to cancel"),
    ];

    let block = Paragraph::new(text)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(block, centered_rect(70, 70, area));
}

fn portfolio_stats(positions: &[Position]) -> (f64, f64, f64) {
    let total_invested: f64 = positions.iter().map(|p| p.invested()).sum();
    let total_proceeds: f64 = positions.iter().map(|p| p.proceeds()).sum();
    let roi_pct = if total_invested.abs() < f64::EPSILON {
        0.0
    } else {
        (total_proceeds - total_invested) / total_invested
    };
    (total_invested, total_proceeds, roi_pct)
}

fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}

fn styled_roi_pct(v: f64) -> Span<'static> {
    let color = if v > 0.0 {
        Color::Green
    } else if v < 0.0 {
        Color::Red
    } else {
        Color::Gray
    };
    Span::styled(format!("{:+.2}%", v * 100.0), Style::default().fg(color))
}

fn bounds_from_points(points: &[(f64, f64)], pad_lo: f64, pad_hi: f64) -> [f64; 2] {
    if points.is_empty() {
        return [-10.0, 10.0];
    }
    let (mut min_y, mut max_y) = (points[0].1, points[0].1);
    for &(_, y) in points.iter().skip(1) {
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let lo = (min_y + pad_lo).floor();
    let hi = (max_y + pad_hi).ceil();
    if (hi - lo).abs() < f64::EPSILON {
        [lo - 1.0, hi + 1.0]
    } else {
        [lo, hi]
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}
