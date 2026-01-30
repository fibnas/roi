use std::{
    error::Error,
    fs,
    io::{self, stdout},
    time::Duration,
};

use chrono::NaiveDate;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use csv::Trim;
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
                        app.editing = None;
                    }
                    KeyCode::Char('i') => {
                        app.mode = Mode::Import;
                        app.import_form = ImportForm::new();
                    }
                    KeyCode::Char('d') | KeyCode::Enter => {
                        if !app.positions.is_empty() {
                            app.mode = Mode::Detail;
                        }
                    }
                    KeyCode::Char('e') => {
                        if let Some(pos) = app.selected_position().cloned() {
                            app.mode = Mode::AddForm;
                            app.editing = Some(app.selected);
                            app.form = AddForm::from_position(&pos);
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Delete => {
                        app.delete_selected();
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
                    KeyCode::Char('i') => {
                        app.mode = Mode::Import;
                        app.import_form = ImportForm::new();
                    }
                    KeyCode::Char('a') => {
                        app.mode = Mode::AddForm;
                        app.form = AddForm::new();
                        app.editing = None;
                    }
                    KeyCode::Char('e') => {
                        if let Some(pos) = app.selected_position().cloned() {
                            app.mode = Mode::AddForm;
                            app.editing = Some(app.selected);
                            app.form = AddForm::from_position(&pos);
                        }
                    }
                    KeyCode::Char('x') | KeyCode::Delete => {
                        app.delete_selected();
                        app.mode = Mode::Portfolio;
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
                Mode::Import => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Portfolio;
                        app.import_form = ImportForm::new();
                    }
                    KeyCode::Enter => {
                        let path = app.import_form.path.trim().to_string();
                        if path.is_empty() {
                            app.import_form.error = Some("Path cannot be empty".into());
                        } else {
                            match app.import_csv(&path) {
                                Ok(count) => {
                                    app.import_form.message =
                                        Some(format!("Imported {count} positions"));
                                    app.import_form.error = None;
                                    app.mode = Mode::Portfolio;
                                }
                                Err(err) => {
                                    app.import_form.error = Some(err);
                                    app.import_form.message = None;
                                }
                            }
                        }
                    }
                    KeyCode::Backspace => app.import_form.backspace(),
                    KeyCode::Char(c) => {
                        if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                            app.import_form.push_char(c);
                        }
                    }
                    _ => {}
                },
                Mode::AddForm => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Portfolio;
                        app.editing = None;
                        app.form.error = None;
                    }
                    KeyCode::Enter => {
                        if app.form.on_enter() {
                            match app.form.try_build_position() {
                                Ok(pos) => {
                                    if let Some(idx) = app.editing {
                                        app.positions[idx] = pos;
                                        app.selected = idx;
                                    } else {
                                        app.positions.push(pos);
                                        app.selected = app.positions.len().saturating_sub(1);
                                    }
                                    app.mode = Mode::Portfolio;
                                    app.editing = None;
                                    app.form.error = None;
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
    ticker: String,
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
                Field::new("Ticker", "e.g. AAPL"),
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

    fn from_position(pos: &Position) -> Self {
        let mut form = Self::new();
        form.fields[0].value = pos.ticker.clone();
        form.fields[1].value = format!("{:.2}", pos.cost_per_share);
        form.fields[2].value = format!("{:.4}", pos.quantity);
        form.fields[3].value = format!("{:.2}", pos.sale_price);
        form.fields[4].value = pos.purchase_date.format(DATE_FMT).to_string();
        form.fields[5].value = pos.sale_date.format(DATE_FMT).to_string();
        form
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
        let ticker = parse_ticker(&self.fields[0].value)?;
        let cost = parse_f64(&self.fields[1].value, "cost/share")?;
        let qty = parse_f64(&self.fields[2].value, "quantity")?;
        let sale_price = parse_f64(&self.fields[3].value, "sale price")?;
        let purchase_date = parse_date(&self.fields[4].value, "purchase date")?;
        let sale_date = parse_date(&self.fields[5].value, "sale date")?;

        if sale_date < purchase_date {
            return Err("Sale date cannot be before purchase date".into());
        }

        Ok(Position {
            ticker,
            cost_per_share: cost,
            quantity: qty,
            sale_price,
            purchase_date,
            sale_date,
        })
    }
}

#[derive(Clone)]
struct ImportForm {
    path: String,
    message: Option<String>,
    error: Option<String>,
}

impl ImportForm {
    fn new() -> Self {
        Self {
            path: String::new(),
            message: None,
            error: None,
        }
    }

    fn backspace(&mut self) {
        self.path.pop();
    }

    fn push_char(&mut self, c: char) {
        self.path.push(c);
    }
}

fn parse_f64(raw: &str, label: &str) -> Result<f64, String> {
    parse_number(raw).ok_or_else(|| format!("Invalid {label}"))
}

fn parse_date(raw: &str, label: &str) -> Result<NaiveDate, String> {
    parse_date_any(raw).map_err(|_| format!("Invalid {label}, expected YYYY-MM-DD or MM/DD/YYYY"))
}

fn parse_ticker(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Ticker cannot be empty".into());
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn parse_date_any(raw: &str) -> Result<NaiveDate, ()> {
    let trimmed = raw.trim();
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(trimmed, "%m/%d/%Y"))
        .map_err(|_| ())
}

fn parse_number(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "--" {
        return None;
    }
    let mut cleaned = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch == ',' || ch == '$' || ch == ' ' {
            continue;
        }
        cleaned.push(ch);
    }
    cleaned.parse::<f64>().ok()
}

fn parse_positions_csv(path: &str) -> Result<Vec<Position>, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;

    #[derive(Clone, Copy)]
    struct HeaderIdx {
        ticker: usize,
        cost: usize,
        qty: usize,
        sale_price: usize,
        buy_date: usize,
        sale_date: usize,
    }

    fn sanitize_header(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }

    fn detect_header(parts: &[String]) -> Option<HeaderIdx> {
        let mut t = None;
        let mut cost = None;
        let mut qty = None;
        let mut sale = None;
        let mut buy_d = None;
        let mut sale_d = None;
        let mut date_cols: Vec<usize> = Vec::new();

        for (i, raw) in parts.iter().enumerate() {
            let h = sanitize_header(raw);
            match h.as_str() {
                "symbol" | "ticker" => t = Some(i),
                "qty" | "qtynumber" | "qtyshare" | "quantity" | "qtyshares" => qty = Some(i),
                "costshare" | "costpershare" => cost = Some(i),
                "priceshare" | "pricepershare" | "saleprice" | "sellprice" => sale = Some(i),
                "dateadded" | "purchasedate" | "buydate" => buy_d = Some(i),
                "date" | "saledate" | "selldate" => date_cols.push(i),
                _ => {}
            }
        }

        if buy_d.is_none() {
            if let Some(first_date) = date_cols.get(0) {
                buy_d = Some(*first_date);
            }
        }
        if sale_d.is_none() {
            if let Some(second_date) = date_cols.get(1) {
                sale_d = Some(*second_date);
            } else if let Some(first_date) = date_cols.get(0) {
                sale_d = Some(*first_date);
            }
        }

        match (t, cost, qty, sale, buy_d, sale_d) {
            (Some(t), Some(c), Some(q), Some(s), Some(bd), Some(sd)) => Some(HeaderIdx {
                ticker: t,
                cost: c,
                qty: q,
                sale_price: s,
                buy_date: bd,
                sale_date: sd,
            }),
            _ => None,
        }
    }

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .trim(Trim::All)
        .flexible(true)
        .from_reader(data.as_bytes());

    let mut header_idx: Option<HeaderIdx> = None;
    let mut positions = Vec::new();
    let mut in_details_section = false;
    let mut current_ticker: Option<String> = None;

    for (idx, result) in rdr.records().enumerate() {
        let line_no = idx + 1;
        let record = result.map_err(|e| format!("Line {line_no}: {e}"))?;
        if record.is_empty() {
            continue;
        }

        let fields: Vec<String> = record.iter().map(|s| s.to_string()).collect();
        let joined_lower = fields.join(" ").to_ascii_lowercase();
        if joined_lower.contains("taxable g&l details") {
            in_details_section = true;
            header_idx = None;
            continue;
        }

        // Skip anything before we reach the TAXABLE G&L DETAILS table.
        if !in_details_section && header_idx.is_none() {
            continue;
        }

        // Skip summary/total lines but keep headers that include the word "Total"
        if fields.len() == 1 {
            let first = fields[0].trim().to_ascii_lowercase();
            if first.contains("total") || first.contains("subtotal") {
                continue;
            }
        }
        if let Some(first) = fields.get(0) {
            let first_lower = first.trim().to_ascii_lowercase();
            if first_lower == "total" || first_lower == "subtotal" {
                continue;
            }
        }

        if header_idx.is_none() {
            if let Some(h) = detect_header(&fields) {
                header_idx = Some(h);
                continue;
            }
            // Not a header row; ignore until we find one.
            continue;
        }

        let get = |i: usize| fields.get(i).map(|s| s.as_str()).unwrap_or("");

        let push_position = |ticker: String,
                             cost: f64,
                             qty: f64,
                             sale_price: f64,
                             purchase_date: NaiveDate,
                             sale_date: NaiveDate,
                             positions: &mut Vec<Position>| {
            positions.push(Position {
                ticker,
                cost_per_share: cost,
                quantity: qty,
                sale_price,
                purchase_date,
                sale_date,
            });
        };

        if let Some(h) = header_idx {
            let raw_ticker = get(h.ticker).trim();
            // Update current ticker when we see a non-sell summary row, even if numbers are missing.
            if !raw_ticker.is_empty()
                && raw_ticker != "--"
                && !raw_ticker.to_ascii_lowercase().starts_with("sell")
            {
                let parsed =
                    parse_ticker(raw_ticker).map_err(|e| format!("Line {line_no}: {e}"))?;
                current_ticker = Some(parsed);
            }

            let required_missing = |i: usize| {
                let v = get(i).trim();
                v.is_empty() || v == "--"
            };
            if required_missing(h.cost)
                || required_missing(h.qty)
                || required_missing(h.sale_price)
                || required_missing(h.buy_date)
                || required_missing(h.sale_date)
            {
                continue;
            }

            let ticker = if let Some(t) = &current_ticker {
                t.clone()
            } else {
                continue; // no context yet
            };
            let cost =
                parse_f64(get(h.cost), "cost/share").map_err(|e| format!("Line {line_no}: {e}"))?;
            let qty =
                parse_f64(get(h.qty), "quantity").map_err(|e| format!("Line {line_no}: {e}"))?;
            let sale_price = parse_f64(get(h.sale_price), "sale price")
                .map_err(|e| format!("Line {line_no}: {e}"))?;
            let purchase_date = parse_date(get(h.buy_date), "purchase date")
                .map_err(|e| format!("Line {line_no}: {e}"))?;
            let sale_date = parse_date(get(h.sale_date), "sale date")
                .map_err(|e| format!("Line {line_no}: {e}"))?;

            if sale_date < purchase_date {
                return Err(format!(
                    "Line {line_no}: sale date cannot be before purchase date"
                ));
            }

            push_position(
                ticker,
                cost,
                qty,
                sale_price,
                purchase_date,
                sale_date,
                &mut positions,
            );
            continue;
        }

        // Fallback: expect at least 6 columns in ticker,cost,qty,sale,purchase_date,sale_date order
        if fields.len() < 6 {
            // pre/post table fluff; skip
            continue;
        }

        let raw_ticker = get(0).trim();
        // Update current ticker from summary rows, skip adding a position for them
        if !raw_ticker.is_empty()
            && raw_ticker != "--"
            && !raw_ticker.to_ascii_lowercase().starts_with("sell")
        {
            let parsed = parse_ticker(raw_ticker).map_err(|e| format!("Line {line_no}: {e}"))?;
            current_ticker = Some(parsed);
            continue;
        }

        let required_missing = |s: &str| {
            let t = s.trim();
            t.is_empty() || t == "--"
        };
        if required_missing(get(1))
            || required_missing(get(2))
            || required_missing(get(3))
            || required_missing(get(4))
            || required_missing(get(5))
        {
            continue;
        }

        let ticker = if let Some(t) = &current_ticker {
            t.clone()
        } else {
            continue;
        };
        let cost = parse_f64(get(1), "cost/share").map_err(|e| format!("Line {line_no}: {e}"))?;
        let qty = parse_f64(get(2), "quantity").map_err(|e| format!("Line {line_no}: {e}"))?;
        let sale_price =
            parse_f64(get(3), "sale price").map_err(|e| format!("Line {line_no}: {e}"))?;
        let purchase_date =
            parse_date(get(4), "purchase date").map_err(|e| format!("Line {line_no}: {e}"))?;
        let sale_date =
            parse_date(get(5), "sale date").map_err(|e| format!("Line {line_no}: {e}"))?;

        if sale_date < purchase_date {
            return Err(format!(
                "Line {line_no}: sale date cannot be before purchase date"
            ));
        }

        push_position(
            ticker,
            cost,
            qty,
            sale_price,
            purchase_date,
            sale_date,
            &mut positions,
        );
    }

    if positions.is_empty() {
        return Err("No rows found to import".into());
    }
    Ok(positions)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Portfolio,
    Detail,
    AddForm,
    Import,
    Help,
}

struct App {
    positions: Vec<Position>,
    selected: usize,
    mode: Mode,
    form: AddForm,
    import_form: ImportForm,
    editing: Option<usize>,
}

impl App {
    fn new() -> Self {
        let today = chrono::Utc::now().date_naive();
        Self {
            positions: vec![
                Position {
                    ticker: "AAPL".into(),
                    cost_per_share: 110.0,
                    quantity: 40.0,
                    sale_price: 127.5,
                    purchase_date: today - chrono::Days::new(12),
                    sale_date: today,
                },
                Position {
                    ticker: "AMD".into(),
                    cost_per_share: 64.0,
                    quantity: 100.0,
                    sale_price: 59.4,
                    purchase_date: today - chrono::Days::new(4),
                    sale_date: today,
                },
                Position {
                    ticker: "MSFT".into(),
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
            import_form: ImportForm::new(),
            editing: None,
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

    fn delete_selected(&mut self) {
        if self.positions.is_empty() {
            return;
        }
        self.positions.remove(self.selected);
        if self.positions.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.positions.len() {
            self.selected = self.positions.len() - 1;
        }
    }

    fn import_csv(&mut self, path: &str) -> Result<usize, String> {
        let start = self.positions.len();
        let new_positions = parse_positions_csv(path)?;
        self.positions.extend(new_positions);
        if !self.positions.is_empty() {
            self.selected = self.positions.len() - 1;
        }
        Ok(self.positions.len() - start)
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
        Mode::Import => draw_import_form(f, size, app),
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
        Mode::Portfolio => {
            "↑/↓ select  • enter/d detail  • a add  • e edit  • x delete  • i import  • h help  • q quit"
        }
        Mode::Detail => {
            "↑/↓ move  • b/esc back  • e edit  • x delete  • a add  • i import  • q quit"
        }
        Mode::AddForm => "tab/shift+tab move  • enter next/save  • esc cancel",
        Mode::Import => "type path  • enter import  • esc cancel",
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
        "Pos", "Ticker", "Cost", "Qty", "Sale", "PnL$", "ROI%", "Days", "Bought", "Sold",
    ])
    .style(Style::default().fg(Color::Yellow));

    let rows: Vec<Row> = app
        .positions
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let pnl = Cell::from(Span::styled(
                format_currency(p.roi_value()),
                Style::default().fg(if p.roi_value() >= 0.0 {
                    Color::Green
                } else {
                    Color::Red
                }),
            ));
            let roi = Cell::from(styled_roi_pct(p.roi_pct()));
            Row::new(vec![
                Cell::from(format!("#{idx}")),
                Cell::from(p.ticker.as_str()),
                Cell::from(format_currency(p.cost_per_share)),
                Cell::from(format!("{:.2}", p.quantity)),
                Cell::from(format_currency(p.sale_price)),
                pnl,
                roi,
                Cell::from(p.days_held().to_string()),
                Cell::from(p.purchase_date.format(DATE_FMT).to_string()),
                Cell::from(p.sale_date.format(DATE_FMT).to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(10),
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
            Span::styled("Ticker ", Style::default().fg(Color::Gray)),
            Span::styled(pos.ticker.as_str(), Style::default().fg(Color::Yellow)),
        ]),
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
    let title = if app.editing.is_some() {
        "Edit position"
    } else {
        "Add position"
    };
    let block = Block::default()
        .title(title)
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

fn draw_import_form(f: &mut Frame, area: Rect, app: &App) {
    let form_area = centered_rect(70, 40, area);
    let block = Block::default()
        .title("Import from CSV (ticker,cost,qty,sale,purchase_date,sale_date)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    f.render_widget(block, form_area);

    let inner = form_area.inner(&ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::Gray)),
            Span::raw(app.import_form.path.as_str()),
        ]),
        Line::from("Press Enter to import, Esc to cancel"),
    ];

    if let Some(msg) = &app.import_form.message {
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::Green),
        )));
    }
    if let Some(err) = &app.import_form.error {
        lines.push(Line::from(Span::styled(
            err,
            Style::default().fg(Color::Red),
        )));
    }

    let para = Paragraph::new(lines).block(Block::default());
    f.render_widget(para, inner);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("ROI Tracker TUI"),
        Line::from(" "),
        Line::from("Portfolio view:"),
        Line::from("  - ↑/↓ move selection"),
        Line::from("  - enter/d open position detail"),
        Line::from("  - a add  • e edit  • x delete  • i import CSV"),
        Line::from("  - h open this help, q quit"),
        Line::from(" "),
        Line::from("Form view:"),
        Line::from("  - tab / shift+tab to move"),
        Line::from("  - enter to advance or save on last field"),
        Line::from("  - esc to cancel"),
        Line::from(" "),
        Line::from("Import view:"),
        Line::from("  - type CSV path, enter to import, esc to cancel"),
        Line::from("  - columns: ticker,cost,qty,sale,purchase_date,sale_date"),
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
