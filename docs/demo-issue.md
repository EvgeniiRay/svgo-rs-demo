<!-- The issue for the GitHub Actions demo. File it as written, then add the `muse-fix` label. -->

Title: convertColors turns #aabbcd into #abc

`svgo-rs` changes the color of this icon. SVGO keeps it:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z" fill="#aabbcd"/></svg>
```

- SVGO 4.1.0: `fill="#aabbcd"`
- svgo-rs: `fill="#abc"`, which is a different color (`#aabbcc`)

It looks like the short-hex step in convertColors shortens colors whose last two digits differ.
`rgb(170, 187, 205)` has the same problem, because it becomes `#aabbcd` first.
