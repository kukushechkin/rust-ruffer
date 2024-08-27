# Ruff Fixer

Ruff Fixer is a Rust command-line tool designed to automatically identify and fix issues in Python code using the Ruff tool and OpenAI's ChatGPT API. This tool runs Ruff checks on a specified root folder, identifies issues in Python files, and then uses ChatGPT to provide fixes for those issues.

## Features

- Runs Ruff checks on a specified directory.
- Groups issues by file and sends each file's issues to OpenAI's ChatGPT API for fixes.
- Asynchronously applies the fixes to the affected files.

## Prerequisites

- Rust and Cargo installed ([Install Rust](https://www.rust-lang.org/tools/install)).
- The `ruff` tool installed and available in your system's PATH ([Ruff installation guide](https://github.com/charliermarsh/ruff)).
- OpenAI API key for accessing the ChatGPT API.

## Installation

1. Clone the repository:

    ```bash
    git clone https://github.com/kukushechkin/ruff_fixer.git
    cd ruff_fixer
    ```

2. Build the project using Cargo:

    ```bash
    cargo build --release
    ```

    This will create an executable file in the `target/release` directory.

## Usage

To run Ruff Fixer, use the following command:

```bash
cargo run -- <api_key> <ruff_path> <root_folder>
