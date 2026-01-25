# TODO

---

## Data Issues

### Orphaned References in Repository Libraries

**Severity:** Low (data issue, not code bug)

The repository's existing libraries (e.g., `mcus.PcbLib`) have `component_bodies` referencing
STEP models without embedded data. Use `repair_library` to fix:

```text
repair_library(filepath, dry_run=false)
```

---

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
