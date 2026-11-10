# Fix what the site audit found

You run unattended in GitHub Actions on a pull request. Nobody will answer
questions.

`site/` is the project's docs site, a few static pages served as they are by
GitHub Pages. The site audit (Lighthouse on every page, plus a check of every
local link) failed on this pull request. Its score table and every failed check
follow this prompt.

1. Fix every failed check in `site/`: search, accessibility, best practices and
   performance. Keep the pages' content, look and layout; fix the markup, the
   styles and the assets.
2. Make the pages light. This repository builds svgo-rs, an SVG optimizer
   (`cargo build --release`, then `./target/release/svgo-rs --help`), if you
   need one.
3. Run the audit again yourself until it passes:
   `node scripts/ci/site-audit.mjs site audit-out`. If Lighthouse cannot start
   here, fix from the report below; the workflow runs the audit again.
4. Do not commit or push; the workflow does that.

Your final answer must follow the JSON schema you were given.
