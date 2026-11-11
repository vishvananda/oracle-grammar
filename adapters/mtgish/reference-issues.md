# Reference issues

Confirmed disagreements where the pinned MTGish reference, not the grammar, is
wrong. Gold is never edited locally. Each `###` heading names one face; corpus
comparisons accept a lost match only for a face listed here that still parses.
After an upstream fix, advance `reference.json`, re-prepare the inputs and
recount. Paths are IR paths inside the face's ability list.

Revision for every entry unless noted: `1cd0b57efcf32b842557a9bb74b985df98da3daa`.

## Dropped "and only as a sorcery" activation limit

"Activate only if <condition> and only as a sorcery." sets both the condition and
the sorcery-speed limit (CR 602.5d). The reference keeps the condition but sets
`sorcery: false` for two faces, while Uchbenbak, the Great Mistake encodes the
same sentence with `sorcery: true`.

### Balustrade Wurm

- At `[3].args.ability.args.limits.sorcery`: reference `false`, expected `true`.
- Status: open.

### Resurrected Cultist

- At `[0].args.ability.args.limits.sorcery`: reference `false`, expected `true`.
- Status: open.
