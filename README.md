# gfr: Grep-Find-Rust ⚡

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/gfr.svg)](https://crates.io/crates/gfr)
[![Build Status](https://github.com/Kr1shna4garwal/gfr/workflows/Build%20and%20Release%20gfr/badge.svg)](https://github.com/Kr1shna4garwal/gfr/actions)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-brightgreen.svg)](https://www.rust-lang.org)

**`gfr`** is a blazingly-fast, pure Rust command-line tool for finding patterns in your code, inspired by `gf` (https://github.com/tomnomnom/gf). It leverages pre-defined, community-contributed, or your own custom JSON pattern files to rapidly scan for anything.

Built on the high-performance core of `ripgrep`, `gfr` is designed for speed and efficiency.

---

## Features

-   **Pattern-Based Searching**: Define what you're looking for in simple JSON files, just like the Gf patterns (Same concept).
-   **Blazing Fast**: Uses `ripgrep`'s libraries for lightning-fast directory traversal and searching.
-   **Installable Patterns**: Download and update curated pattern collections with a single command (`gfr install`).
-   **Advanced Filtering**: Search using a specific pattern name, or filter all patterns by `tags` and `author`.
-   **Stdin & File Support**: Pipe output from other tools or search directories directly.
-   **Customizable**: Easily create and save your own patterns with `gfr save`.

---

## Installation

### From [crates.io](https://crates.io/crates/gfr)
```bash
cargo install gfr
```

### From Git (latest main branch)

```bash
cargo install --git https://github.com/Kr1shna4garwal/gfr
```

### From source (local development)

```bash
# Clone the repository
git clone https://github.com/Kr1shna4garwal/gfr.git
cd gfr

# Build and install the binary
cargo install --path .

# Verify installation
gfr --version
```

---

## Usage

### Installing and Listing Patterns

The best way to start is by installing a collection of community patterns.

```bash
# Install the default set of patterns
gfr install

# List all locally available patterns
gfr list
```

This will download patterns into your system's config directory (`~/Library/Application Support/gfr` on MacOS).

### Searching for Code

You can search using a specific pattern by its name, or by filtering with tags/author.

```bash
# Search for potential secrets in the current directory
gfr search secrets

# Search for both XSS and secrets by tag in a specific project
# This combines all patterns tagged with "security" into one search
gfr search --tags security ./

# Search for all patterns by a specific author
gfr search --author "Krishna Agarwal <kr1shna4garwal@proton.me>"
```

### Creating Your Own Patterns

You can easily create your own local patterns.

```bash
gfr save my-pattern "my_regex" \
    --description "Light the fuse!" \
    --file-types "rs,toml" \
    --author "EthanHunt" \
    --tags "custom,project-x"
```

### Other Commands

```bash
# See the configuration for a specific pattern without searching
gfr search --dump secrets

# Get help for any command
gfr --help
gfr search --help
```

---

## Pattern File Structure

A `gfr` pattern is a simple JSON file, just like the gf patterns. Here is a example:

```json
{
  "version": "1.0.0",
  "author": "Krishna Agarwal <kr1shna4garwal@proton.me>",
  "description": "Finds common secret keys, API tokens, and credentials.",
  "tags": ["security", "credentials", "secrets"],
  "patterns": [
    "[Aa][Pp][Ii]_?[Kk][Ee][Yy]",
    "[Ss][Ee][Cc][Rr][Ee][Tt]_?[Kk][Ee][Yy]"
  ],
  "file_types": ["js", "py", "yml", "env"],
  "ignore_case": false,
  "multiline": false
}
```
-   **`version`**: Semantic version.
-   **`author`**: (Optional) The pattern's author.
-   **`description`**: (Optional) A short explanation.
-   **`tags`**: (Optional) A list of strings for categorization.
-   **`pattern`** or **`patterns`**: A single regex string or a list of regex strings.
-   **`file_types`**: (Optional) A list of file extensions to search in.
-   **`ignore_case`**: (Optional `bool`) Enables case-insensitive matching.
-   **`multiline`**: (Optional `bool`) Enables regex `.` to match newlines.

---

## FAQ

**Q:** Why do we need another such tool when one already exists?
**A:** I made this as side project, gf is awesome, I used it a lot in early days, but now, I feel it a bit backward and slow. So I made this.

**Q:** How can I create my own patterns index?
**A:** See https://github.com/Kr1shna4garwal/gfr-patterns

**Q:** I don't understand regex, I have target specific search candidates, what to do now? 🙁
**A:** No problem, You can use https://regex.fav83.com, It allows you to generate regex patterns using LLM quickly. More better, try learning regex, I know regex sucks, but it's not rocket science :)

**Q:** gfr?
**A:** grep-find-rust

**Q:** I wish to contribute patterns to default index
**A:** You can contribute to default index, Please check https://github.com/kr1shna4garwal/gfr-patterns/blob/main/CONTRIBUTING.md :)

**Q:** I got a question for you, can I ask personally?
**A:** Yes, you can contact me at https://kr1shna4garwal.com/contact

## Contribution

I welcome contributions from the community to make **\[gfr]** better.

### Steps to Contribute

1. **Fork** the repository.
2. Create a **new branch**:

   ```bash
   git checkout -b feature/your-feature-name
   ```

3. Run **clippy and fmt** on your codechanges:

  ```bash
  cargo clippy
  cargo fmt
  ```

4. **Commit** your changes with a clear message:

   ```bash
   git commit -m "Add: detailed description of your change"
   ```
5. **Push** to your branch:

   ```bash
   git push origin feature/your-feature-name
   ```
6. Open a **Pull Request** to the `main` branch.

> Make sure your code follows the project’s coding style.


## License

This project is licensed under the **MIT License**. See the `LICENSE` file for details.