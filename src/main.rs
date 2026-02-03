use std::{
    error::Error,
    fs,
    io::{self, stdout},
    path::Path,
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
use serde::{Deserialize, Serialize};

const DATE_FMT: &str = "%Y-%m-%d";
const DATA_FILE: &str = "positions.json";

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
            Event::Key(key) => {
                if app.filter_editing {
                    match key.code {
                        KeyCode::Esc => app.filter_editing = false,
                        KeyCode::Enter => app.filter_editing = false,
                        KeyCode::Backspace => {
                            app.filter_text.pop();
                            app.ensure_selection_visible();
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                                app.filter_text.push(c);
                                app.ensure_selection_visible();
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                match app.mode {
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
                            if !app.filtered_positions().is_empty() {
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
                        KeyCode::Char('f') | KeyCode::Char('/') => {
                            app.filter_editing = true;
                        }
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
                        KeyCode::Char('f') | KeyCode::Char('/') => {
                            app.filter_editing = true;
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
                                        app.ensure_selection_visible();
                                        save_positions(&app.positions);
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
                }
            }
            Event::Resize(_, _) => {} // redraw happens next loop
            _ => {}
        }
    }

    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

        if buy_d.is_none()
            && let Some(&first_date) = date_cols.first()
        {
            buy_d = Some(first_date);
        }
        if sale_d.is_none() {
            if let Some(second_date) = date_cols.get(1) {
                sale_d = Some(*second_date);
            } else if let Some(&first_date) = date_cols.first() {
                sale_d = Some(first_date);
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
        if let Some(first) = fields.first() {
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

fn load_positions() -> Result<Vec<Position>, String> {
    let path = Path::new(DATA_FILE);
    if !path.exists() {
        return Err("no data file".into());
    }
    let data = fs::read_to_string(path).map_err(|e| format!("Failed to read data file: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse data file: {e}"))
}

fn save_positions(positions: &[Position]) {
    let path = Path::new(DATA_FILE);
    if let Ok(json) = serde_json::to_string_pretty(positions)
        && let Err(err) = fs::write(path, json)
    {
        eprintln!("Could not save positions: {err}");
    }
}

fn seed_positions() -> Vec<Position> {
    let today = chrono::Utc::now().date_naive();
    vec![
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
    ]
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
    filter_text: String,
    filter_editing: bool,
}

impl App {
    fn new() -> Self {
        let positions = load_positions().unwrap_or_else(|_| seed_positions());
        let selected = if positions.is_empty() {
            0
        } else {
            positions.len() - 1
        };
        Self {
            positions,
            selected,
            mode: Mode::Portfolio,
            form: AddForm::new(),
            import_form: ImportForm::new(),
            editing: None,
            filter_text: String::new(),
            filter_editing: false,
        }
    }

    fn select_next(&mut self) {
        let filtered = self.filtered_indices();
        if filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let current = filtered
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = (current + 1) % filtered.len();
        self.selected = filtered[next];
    }

    fn select_prev(&mut self) {
        let filtered = self.filtered_indices();
        if filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let current = filtered
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let prev = if current == 0 {
            filtered.len() - 1
        } else {
            current - 1
        };
        self.selected = filtered[prev];
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
        if !self.positions.is_empty() {
            self.ensure_selection_visible();
        }
        save_positions(&self.positions);
    }

    fn import_csv(&mut self, path: &str) -> Result<usize, String> {
        let start = self.positions.len();
        let new_positions = parse_positions_csv(path)?;
        self.positions.extend(new_positions);
        if !self.positions.is_empty() {
            self.selected = self.positions.len() - 1;
        }
        self.ensure_selection_visible();
        save_positions(&self.positions);
        Ok(self.positions.len() - start)
    }

    fn filter_matches(&self, pos: &Position) -> bool {
        if self.filter_text.is_empty() {
            return true;
        }
        let needle = self.filter_text.to_ascii_uppercase();
        pos.ticker.to_ascii_uppercase().contains(&needle)
    }

    fn filtered_positions(&self) -> Vec<(usize, &Position)> {
        self.positions
            .iter()
            .enumerate()
            .filter(|(_, p)| self.filter_matches(p))
            .collect()
    }

    fn filtered_indices(&self) -> Vec<usize> {
        self.filtered_positions()
            .into_iter()
            .map(|(i, _)| i)
            .collect()
    }

    fn ensure_selection_visible(&mut self) {
        let filtered = self.filtered_indices();
        if filtered.is_empty() {
            self.selected = 0;
            return;
        }
        if !filtered.contains(&self.selected) {
            self.selected = filtered[0];
        }
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
            "↑/↓ select  • enter/d detail  • f filter  • a add  • e edit  • x delete  • i import  • h help  • q quit"
        }
        Mode::Detail => {
            "↑/↓ move  • f filter  • b/esc back  • e edit  • x delete  • a add  • i import  • q quit"
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
    let filtered = app.filtered_positions();
    let header = Row::new(vec![
        "Pos", "Ticker", "Cost", "Qty", "Sale", "PnL$", "ROI%", "Days", "Bought", "Sold",
    ])
    .style(Style::default().fg(Color::Yellow));

    let mut rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(display_idx, (_, p))| {
            let pnl_val = p.roi_value();
            let pnl = Cell::from(Span::styled(
                format_currency(pnl_val),
                Style::default().fg(if pnl_val >= 0.0 {
                    Color::Green
                } else {
                    Color::Red
                }),
            ));
            let roi = Cell::from(styled_roi_pct(p.roi_pct()));
            Row::new(vec![
                Cell::from(format!("#{}", display_idx + 1)),
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

    let summary =
        summarize_positions(&filtered.iter().map(|(_, p)| *p).collect::<Vec<&Position>>());

    let mut summary_rows = Vec::new();
    let avg_pnl = Cell::from(Span::styled(
        format_currency(summary.avg_pnl),
        Style::default().fg(if summary.avg_pnl >= 0.0 {
            Color::Green
        } else {
            Color::Red
        }),
    ));
    summary_rows.push(
        Row::new(vec![
            Cell::from(""),
            Cell::from(Span::styled(
                "Avg",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            avg_pnl,
            Cell::from(styled_roi_pct(summary.avg_roi_pct)),
            Cell::from(format!("{:.1}", summary.avg_days)),
            Cell::from(""),
            Cell::from(""),
        ])
        .style(Style::default().bg(Color::DarkGray)),
    );

    let total_pnl = Cell::from(Span::styled(
        format_currency(summary.total_pnl),
        Style::default().fg(if summary.total_pnl >= 0.0 {
            Color::Green
        } else {
            Color::Red
        }),
    ));
    summary_rows.push(
        Row::new(vec![
            Cell::from(""),
            Cell::from(Span::styled(
                "Total",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            total_pnl,
            Cell::from(styled_roi_pct(summary.weighted_roi_pct)),
            Cell::from(summary.total_days.to_string()),
            Cell::from(""),
            Cell::from(""),
        ])
        .style(Style::default().bg(Color::DarkGray)),
    );

    rows.extend(summary_rows);

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

    let mut title = "Positions".to_string();
    if app.filter_editing {
        title.push_str(" – filter: typing...");
    } else if !app.filter_text.is_empty() {
        title.push_str(&format!(" – filter: {}", app.filter_text));
    } else {
        title.push_str(" – press f to filter");
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = TableState::default();
    let selected_row = filtered.iter().position(|(idx, _)| *idx == app.selected);
    state.select(selected_row);
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_portfolio_chart(f: &mut Frame, area: Rect, app: &App) {
    let filtered = app.filtered_positions();
    let points: Vec<(f64, f64)> = filtered
        .iter()
        .enumerate()
        .map(|(i, (_, p))| (i as f64, p.roi_pct() * 100.0))
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
        Line::from("  - f start ticker filter; type to refine, enter/esc to exit"),
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

#[derive(Default)]
struct PositionSummary {
    total_pnl: f64,
    avg_pnl: f64,
    avg_roi_pct: f64,
    weighted_roi_pct: f64,
    total_days: i64,
    avg_days: f64,
}

fn summarize_positions(positions: &[&Position]) -> PositionSummary {
    let count = positions.len();
    if count == 0 {
        return PositionSummary::default();
    }

    let total_pnl = positions.iter().map(|p| p.roi_value()).sum::<f64>();
    let avg_pnl = total_pnl / count as f64;

    let total_roi = positions.iter().map(|p| p.roi_pct()).sum::<f64>();
    let avg_roi_pct = total_roi / count as f64;

    let total_days = positions.iter().map(|p| p.days_held()).sum::<i64>();
    let avg_days = total_days as f64 / count as f64;

    let total_invested = positions.iter().map(|p| p.invested()).sum::<f64>();
    let total_proceeds = positions.iter().map(|p| p.proceeds()).sum::<f64>();
    let weighted_roi_pct = if total_invested.abs() < f64::EPSILON {
        0.0
    } else {
        (total_proceeds - total_invested) / total_invested
    };

    PositionSummary {
        total_pnl,
        avg_pnl,
        avg_roi_pct,
        weighted_roi_pct,
        total_days,
        avg_days,
    }
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
