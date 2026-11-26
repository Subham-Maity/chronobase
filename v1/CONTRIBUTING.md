# Contributing to ChronoBase

Thanks for taking the time to contribute!

## Getting started

```bash
git clone https://github.com/Subham-Maity/chronobase.git
cd chronobase
npm install
node server.js
```

Open http://localhost:3420.

## Project structure

- `src/` — backend modules (database, backup engine, scheduler, routes)
- `public/js/pages/` — one file per UI page
- `public/css/style.css` — all styles in one file

## Guidelines

- Keep modules focused — each file should do one thing
- No commented-out code in PRs
- Match the existing code style (no semicolons at end of module.exports, consistent spacing)
- Test your changes manually before opening a PR

## Reporting bugs

Open an issue with:
1. What you expected to happen
2. What actually happened
3. Your OS and Node.js version
4. Any error messages from the terminal

## Feature requests

Open an issue describing the use case before building it — happy to discuss direction first.
