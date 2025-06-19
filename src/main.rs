use chrono::NaiveDate;
use clap::Parser;
use futures::stream::{self, StreamExt};
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::num::ParseFloatError;
use std::sync::Arc;
use std::time::Duration as StdDuration;

// --- Configuration ---

const API_CALL_DELAY_SECS: u64 = 13; // To stay within 5 calls/minute AlphaVantage free tier limit.

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value_t = -10.0)]
    drop_threshold: f64,

    #[clap(short, long, default_value_t = 5)] // Reduced default due to API limits
    max_concurrent_requests: usize,
}

// --- Data Structures for Alpha Vantage API Response ---

#[derive(Debug, Deserialize)]
struct AlphaVantageResponse {
    #[serde(rename = "Time Series (Daily)")]
    time_series: Option<BTreeMap<NaiveDate, DailyData>>,
    #[serde(rename = "Note")]
    note: Option<String>,
}

// Custom deserialization for string-to-f64 conversion
mod parse_as_f64 {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<f64>().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
struct DailyData {
    #[serde(rename = "1. open", with = "parse_as_f64")]
    open: f64,
    #[serde(rename = "2. high", with = "parse_as_f64")]
    high: f64,
    #[serde(rename = "3. low", with = "parse_as_f64")]
    low: f64,
    #[serde(rename = "4. close", with = "parse_as_f64")]
    close: f64,
    #[serde(rename = "5. volume", with = "parse_as_f64")]
    volume: f64,
}

// --- Custom Data Structure for Analysis ---

#[derive(Debug, Serialize, Clone)]
struct StockAnalysis {
    symbol: String,
    current_price: f64,
    previous_close: f64,
    percent_change: f64,
    date: NaiveDate,
}

// --- Error Handling ---

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("HTTP request failed: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("CSV reading failed: {0}")]
    CsvError(#[from] csv::Error),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse float from API response: {0}")]
    ParseFloatError(#[from] ParseFloatError),
    #[error("API Key not found. Please set ALPHAVANTAGE_API_KEY in your .env file.")]
    ApiKeyMissing,
    #[error("API call limit reached or invalid call for {0}: {1}")]
    ApiLimit(String, String),
    #[error("Could not parse data for ticker: {0}")]
    ParsingError(String),
}

// --- Main Application Logic ---

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    dotenvy::dotenv().expect("Failed to read .env file");
    let args = Args::parse();

    let api_key = std::env::var("ALPHAVANTAGE_API_KEY").map_err(|_| AppError::ApiKeyMissing)?;

    let tickers = load_tickers("us_public_tickers.csv")?;
    println!(
        "Loaded {} valid tickers. Starting analysis with {} concurrent requests.",
        tickers.len(),
        args.max_concurrent_requests
    );

    let client = Client::builder()
        .timeout(StdDuration::from_secs(20))
        .build()?;

    let results = analyze_stocks(
        client,
        tickers,
        args.max_concurrent_requests,
        args.drop_threshold,
        api_key,
    )
    .await;

    if results.is_empty() {
        println!("\n🎉 No stocks found with significant drops today!");
    } else {
        display_results(&results);
        save_results_to_csv(&results)?;
    }

    Ok(())
}

fn load_tickers(path: &str) -> Result<Vec<String>, AppError> {
    let file = File::open(path)?;
    let mut rdr = csv::Reader::from_reader(file);
    let tickers: Vec<String> = rdr
        .records()
        .filter_map(|result| result.ok())
        .map(|record| record[0].to_string())
        .filter(|ticker| !ticker.contains('.') && !ticker.contains('$'))
        .collect();
    Ok(tickers)
}

async fn analyze_stocks(
    client: Client,
    tickers: Vec<String>,
    max_concurrent_requests: usize,
    drop_threshold: f64,
    api_key: String,
) -> Vec<StockAnalysis> {
    let api_key = Arc::new(api_key);

    println!("\n--- Starting Stock Analysis ---");
    let all_results: Vec<_> = stream::iter(tickers)
        .map(|ticker| {
            let client = client.clone();
            let api_key = Arc::clone(&api_key);
            tokio::spawn(async move {
                // Delay to respect API rate limits.
                tokio::time::sleep(StdDuration::from_secs(API_CALL_DELAY_SECS)).await;
                let result =
                    fetch_and_analyze_stock(&client, &ticker, drop_threshold, &api_key).await;
                (ticker, result)
            })
        })
        .buffer_unordered(max_concurrent_requests)
        .collect()
        .await;

    let total_tickers = all_results.len();
    let mut successful_analyses = 0;
    let mut failed_analyses = 0;
    let mut dropped_stocks = Vec::new();

    for res in all_results {
        match res {
            Ok((ticker, Ok(Some(analysis)))) => {
                println!(
                    "[DROPPED] {}: Change {:.2}%, Current Price: ${:.2}, Previous Close: ${:.2}",
                    analysis.symbol,
                    analysis.percent_change,
                    analysis.current_price,
                    analysis.previous_close
                );
                successful_analyses += 1;
                dropped_stocks.push(analysis);
            }
            Ok((ticker, Ok(None))) => {
                println!("[INFO] {}: No significant drop detected.", ticker);
                successful_analyses += 1;
            }
            Ok((ticker, Err(e))) => {
                println!("[ERROR] {}: Analysis failed. Reason: {}", ticker, e);
                failed_analyses += 1;
            }
            Err(e) => {
                error!("A spawned task failed: {}", e);
                failed_analyses += 1;
            }
        }
    }

    println!("\n--- Analysis Complete ---");
    println!("Total tickers processed: {}", total_tickers);
    println!("Successful analyses: {}", successful_analyses);
    println!("Failed analyses: {}", failed_analyses);
    println!("Stocks with significant drops: {}", dropped_stocks.len());

    // Sort results by the percentage change, from most dropped to least.
    dropped_stocks.sort_by(|a, b| a.percent_change.partial_cmp(&b.percent_change).unwrap());
    dropped_stocks
}

async fn fetch_and_analyze_stock(
    client: &Client,
    ticker: &str,
    drop_threshold: f64,
    api_key: &str,
) -> Result<Option<StockAnalysis>, AppError> {
    let url = format!(
        "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol={}&apikey={}",
        ticker, api_key
    );

    let res = client
        .get(&url)
        .send()
        .await?
        .json::<AlphaVantageResponse>()
        .await?;

    if let Some(note) = res.note {
        return Err(AppError::ApiLimit(ticker.to_string(), note));
    }

    let time_series = res
        .time_series
        .ok_or_else(|| AppError::ParsingError(ticker.to_string()))?;

    // The BTreeMap is sorted by date, so we can get the two most recent days
    let mut recent_days = time_series.iter().rev(); // Newest first
    let (latest_date, latest_data) = recent_days
        .next()
        .ok_or_else(|| AppError::ParsingError(ticker.to_string()))?;
    let (_, previous_data) = recent_days
        .next()
        .ok_or_else(|| AppError::ParsingError(ticker.to_string()))?;

    let current_price = latest_data.close;
    let previous_close = previous_data.close;

    if previous_close == 0.0 {
        return Ok(None); // Avoid division by zero and meaningless changes.
    }

    let percent_change = ((current_price - previous_close) / previous_close) * 100.0;

    if percent_change <= drop_threshold {
        Ok(Some(StockAnalysis {
            symbol: ticker.to_string(),
            current_price,
            previous_close,
            percent_change,
            date: *latest_date,
        }))
    } else {
        Ok(None)
    }
}

fn display_results(results: &[StockAnalysis]) {
    println!("\n📉 Found {} stocks with significant drops:", results.len());
    println!("{:<10} {:<15} {:<15} {:<15} {:<15}", "Symbol", "Date", "Change %", "Price", "Prev. Close");
    println!("{}", "-".repeat(75));
    for stock in results {
        println!(
            "{:<10} {:<15} {:<15.2}% {:<15.2} {:<15.2}",
            stock.symbol, stock.date.to_string(), stock.percent_change, stock.current_price, stock.previous_close
        );
    }
}

fn save_results_to_csv(results: &[StockAnalysis]) -> Result<(), AppError> {
    let filename = format!("stock_drops_alphavantage_{}.csv", chrono::Utc::now().format("%Y%m%d"));
    let mut wtr = csv::Writer::from_path(&filename)?;
    for result in results {
        wtr.serialize(result)?;
    }
    wtr.flush()?;
    println!("Results saved to {}", filename);
    Ok(())
}