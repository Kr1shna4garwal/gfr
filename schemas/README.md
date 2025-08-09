# GFR JSON Schemas

This directory contains JSON Schema files for validating gfr pattern files and index files.

## Schemas

### `pattern-schema.json`
Validates individual pattern files (e.g., `nodejs_rce.json`, `go_rce.json`).

**Key Features:**
- Enforces semantic versioning for the `version` field
- Validates that either `pattern` OR `patterns` is specified (not both)
- Ensures file extensions don't include dots (e.g., `"js"` not `".js"`)
- Validates regex patterns and structure
- Supports optional fields like `author`, `description`, `tags`, etc.

### `index-schema.json`
Validates pattern index files that list available patterns with their versions and URLs.

**Key Features:**
- Validates pattern names (alphanumeric, underscore, hyphen only)
- Enforces semantic versioning
- Validates URLs (must be HTTP/HTTPS)
- Ensures required fields are present

## Usage

### Adding Schema to Pattern Files

Add the `$schema` property to your pattern JSON files:

```json
{
  "$schema": "./schemas/pattern-schema.json",
  "version": "1.0.0",
  "author": "your-name",
  "description": "Description of what this pattern finds",
  "tags": ["security", "javascript"],
  "pattern": "your-regex-pattern-here",
  "file_types": ["js", "jsx", "ts"],
  "ignore_case": true,
  "multiline": false
}
```

### Adding Schema to Index Files

Add the `$schema` property to your index JSON files:

```json
{
  "$schema": "./schemas/index-schema.json",
  "patterns": [
    {
      "name": "pattern_name",
      "version": "1.0.0",
      "url": "https://example.com/pattern.json"
    }
  ]
}
```

## Pattern File Structure

A pattern file can use either:

1. **Single pattern**: Use the `pattern` field
```json
{
  "version": "1.0.0",
  "pattern": "console\\.log\\s*\\("
}
```

2. **Multiple patterns**: Use the `patterns` array (combined with OR logic)
```json
{
  "version": "1.0.0",
  "patterns": [
    "console\\.log\\s*\\(",
    "console\\.warn\\s*\\(",
    "console\\.error\\s*\\("
  ]
}
```

## Field Descriptions

### Pattern Fields

- `version` (required): Semantic version string (e.g., "1.0.0")
- `author` (optional): Pattern author name
- `description` (optional): What the pattern detects
- `tags` (optional): Array of categorization tags
- `pattern` (either this or `patterns`): Single regex pattern
- `patterns` (either this or `pattern`): Array of regex patterns
- `file_types` (optional): File extensions to search (without dots)
- `ignore_case` (optional, default: false): Case-insensitive search
- `multiline` (optional, default: false): Multi-line regex mode

### Index Fields

- `patterns` (required): Array of pattern objects
  - `name` (required): Pattern identifier (alphanumeric, _, -)
  - `version` (required): Semantic version
  - `url` (required): HTTPS/HTTP URL to pattern file

## Validation

Most editors with JSON Schema support will automatically validate files with the `$schema` property. You can also use tools like `ajv-cli` for command-line validation:

```bash
npm install -g ajv-cli
ajv validate -s schemas/pattern-schema.json -d "*.json"
```

## Examples

See the existing pattern files in the repository root for complete examples:
- `nodejs_rce.json` - Node.js RCE patterns
- `go_rce.json` - Go RCE patterns  
- `rust_rce.json` - Rust RCE patterns
- `ipv4.json` - IPv4 address detection
- `ipv6.json` - IPv6 address detection
