use anyhow::{Context, Result};
use chrono::{Datelike, Local, NaiveDate};
use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use tabled::{
    builder::Builder,
    settings::{Alignment, Height, Modify, Style, Width, object::Segment},
};

#[derive(Parser, Debug)]
#[command(name = "calendered", about = "CLI Calendar Generator")]
struct Cli {
    /// Path to an iCal (.ics) file
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Year to display (defaults to current year)
    #[arg(short, long)]
    year: Option<i32>,

    /// Month to display (1-12) (defaults to current month)
    #[arg(short, long)]
    month: Option<u32>,

    /// Sizing method for the calendar
    #[arg(short = 'S', long, default_value = "auto")]
    sizing: SizingMethod,

    /// Size parameter (applies to 'cell' or 'overall' sizing). Format: WxH, W, Wx, or xH.
    #[arg(short, long)]
    size: Option<Size>,

    /// Truncate event titles instead of wrapping them
    #[arg(short, long)]
    truncate: bool,
}

#[derive(Clone, Debug, Default)]
struct Size {
    w: Option<usize>,
    h: Option<usize>,
}

impl std::str::FromStr for Size {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('x').collect();
        match parts.as_slice() {
            [w_str] => {
                let w = w_str.parse::<usize>().context("Invalid width")?;
                Ok(Size {
                    w: Some(w),
                    h: None,
                })
            }
            [w_str, h_str] => {
                let w = if w_str.is_empty() {
                    None
                } else {
                    Some(w_str.parse::<usize>().context("Invalid width")?)
                };
                let h = if h_str.is_empty() {
                    None
                } else {
                    Some(h_str.parse::<usize>().context("Invalid height")?)
                };
                Ok(Size { w, h })
            }
            _ => anyhow::bail!("Invalid size format, use WxH, W, Wx, or xH"),
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Default, PartialEq)]
enum SizingMethod {
    #[default]
    Auto,
    Cell,
    Overall,
}

fn parse_ical_events(
    path: &PathBuf,
    target_year: i32,
    target_month: u32,
) -> Result<HashMap<u32, Vec<String>>> {
    let file = File::open(path).context("Failed to open iCal file")?;
    let reader = BufReader::new(file);
    let parser = ical::IcalParser::new(reader);

    let mut events_by_day: HashMap<u32, Vec<String>> = HashMap::new();

    for line in parser {
        let calendar = line.context("Failed to parse iCal line")?;
        for event in calendar.events {
            let mut dtstart = None;
            let mut summary = None;

            for prop in event.properties {
                if prop.name == "DTSTART" {
                    dtstart = prop.value;
                } else if prop.name == "SUMMARY" {
                    summary = prop.value;
                }
            }

            if let (Some(dt), Some(sum)) = (dtstart, summary) {
                // simple parsing for standard YYYYMMDDThhmmssZ or YYYYMMDD
                if dt.len() >= 8 {
                    let date_str = &dt[0..8];
                    if let Ok(parsed_date) = NaiveDate::parse_from_str(date_str, "%Y%m%d")
                        && parsed_date.year() == target_year
                        && parsed_date.month() == target_month
                    {
                        events_by_day
                            .entry(parsed_date.day())
                            .or_default()
                            .push(sum);
                    }
                }
            }
        }
    }

    Ok(events_by_day)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let now = Local::now().date_naive();
    let year = cli.year.unwrap_or_else(|| now.year());
    let month = cli.month.unwrap_or_else(|| now.month());

    if !(1..=12).contains(&month) {
        anyhow::bail!("Month must be between 1 and 12");
    }

    let mut events = HashMap::new();
    if let Some(ref path) = cli.file {
        events = parse_ical_events(path, year, month)?;
    }

    // Determine the number of days in the month
    let next_month_year = if month == 12 { year + 1 } else { year };
    let next_month = if month == 12 { 1 } else { month + 1 };

    let first_day = NaiveDate::from_ymd_opt(year, month, 1).context("Invalid date")?;

    let next_month_first_day =
        NaiveDate::from_ymd_opt(next_month_year, next_month, 1).context("Invalid date")?;

    let days_in_month = (next_month_first_day - first_day).num_days() as u32;

    let start_weekday = first_day.weekday().num_days_from_sunday();

    let mut builder = Builder::new();
    builder.push_record(vec!["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]);

    let mut current_row = vec![String::new(); start_weekday as usize];

    for day in 1..=days_in_month {
        let mut cell_content = format!("{}", day);

        if let Some(day_events) = events.get(&day) {
            for event in day_events {
                cell_content.push('\n');
                cell_content.push_str("• ");
                cell_content.push_str(event);
            }
        }

        current_row.push(cell_content);

        if current_row.len() == 7 {
            builder.push_record(current_row.clone());
            current_row.clear();
        }
    }

    if !current_row.is_empty() {
        while current_row.len() < 7 {
            current_row.push(String::new());
        }
        builder.push_record(current_row);
    }

    let mut table = builder.build();
    table.with(Style::modern()).with(Alignment::left());

    match cli.sizing {
        SizingMethod::Auto => {}
        SizingMethod::Cell => {
            if let Some(size) = cli.size {
                if let Some(w) = size.w {
                    table.with(Modify::new(Segment::all()).with(Width::increase(w)));
                    if cli.truncate {
                        table.with(Modify::new(Segment::all()).with(
                            Width::truncate(w).priority(tabled::settings::peaker::Priority::none()),
                        ));
                    } else {
                        table.with(Modify::new(Segment::all()).with(
                            Width::wrap(w).priority(tabled::settings::peaker::Priority::none()),
                        ));
                    }
                }
                if let Some(h) = size.h {
                    table.with(Modify::new(Segment::all()).with(Height::limit(h)));
                }
            }
        }
        SizingMethod::Overall => {
            if let Some(size) = cli.size {
                if let Some(w) = size.w {
                    let borders = 8;
                    let cell_w = w.saturating_sub(borders) / 7;
                    if cell_w > 0 {
                        table.with(Modify::new(Segment::all()).with(Width::increase(cell_w)));
                        if cli.truncate {
                            table.with(
                                Modify::new(Segment::all()).with(
                                    Width::truncate(cell_w)
                                        .priority(tabled::settings::peaker::Priority::none()),
                                ),
                            );
                        } else {
                            table.with(
                                Modify::new(Segment::all()).with(
                                    Width::wrap(cell_w)
                                        .priority(tabled::settings::peaker::Priority::none()),
                                ),
                            );
                        }
                    }
                }
                if let Some(h) = size.h {
                    let rows = table.count_rows();
                    if rows > 0 {
                        let borders = rows + 1; // Assuming 1 border per row + top/bottom
                        let cell_h = h.saturating_sub(borders) / rows;
                        if cell_h > 0 {
                            table.with(Modify::new(Segment::all()).with(Height::limit(cell_h)));
                        }
                    }
                }
            }
        }
    }

    println!("{}", table);

    Ok(())
}
