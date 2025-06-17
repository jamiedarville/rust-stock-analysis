# Rust Stock Analyzer

A command-line tool written in Rust to analyze stock data from the Alpha Vantage API. It identifies stocks that have experienced a significant price drop based on a user-defined threshold and saves the results to a CSV file.

## Features

- Fetches daily stock data from Alpha Vantage.
- Concurrently analyzes multiple stock tickers for efficiency.
- Identifies stocks with significant price drops.
- Displays results in a formatted table in the console.
- Saves the analysis results to a CSV file.
- Configurable drop threshold and concurrency level via command-line arguments.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) installed on your system.

## Setup

1.  **Clone the Repository**:
    ```bash
    git clone <repository_url>
    cd rust-stock-analyzer
    ```

2.  **Create a `.env` File**:
    Create a file named `.env` in the root of the project directory and add your Alpha Vantage API key to it:
    ```
    ALPHAVANTAGE_API_KEY=YOUR_API_KEY
    ```
    You can get a free API key from the [Alpha Vantage website](https://www.alphavantage.co/support/#api-key).

3.  **Ticker List**:
    This tool requires a file named `us_public_tickers.csv` in the root directory, containing a list of stock symbols to analyze. The file should have one ticker symbol per line in the first column.

## Usage

You can build and run the application using `cargo`.

### Running the Application

```bash
cargo run
```

### Command-line Arguments

You can customize the behavior of the tool with the following command-line arguments:

-   `--drop-threshold` (`-d`): The percentage drop to filter for. Stocks that have dropped by this percentage or more will be included in the results. Defaults to `-10.0`.
-   `--max-concurrent-requests` (`-m`): The maximum number of concurrent requests to send to the Alpha Vantage API. The free tier of the API is limited to 5 calls per minute and 100 calls per day. Defaults to `5`.

### Example

To find stocks that have dropped by 5% or more, with a maximum of 3 concurrent requests:

```bash
cargo run -- -d -5.0 -m 3
```

## Output

The application will print a table of the stocks that meet the drop threshold to the console. It will also save the results to a CSV file named `stock_drops_alphavantage_YYYYMMDD.csv` in the project's root directory.