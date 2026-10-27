# Design: <feature-id>

## API and schema changes

- <Endpoints, payloads, events, tables, columns, indexes added/changed/removed.>

## Dependencies

- <New or upgraded packages, services, feature flags, other packets.>

## Migration plan

Required whenever the database schema or persisted data changes; otherwise write
"No migration" and say why. Evidence for this section is checked at the migration
gate (`docs/architecture/release-gate.md`).

- **Expand / contract:** <Expand step (additive, old code still works) and the later
  contract step (removal), each as its own migration and, if needed, its own release.>
- **Ordering vs deploy:** <Migration runs before / after the code deploy, and how it
  is run (deploys do not run migrations automatically).>
- **Backward compatibility:** <Why the currently deployed code keeps working on the
  new schema; how this is tested.>
- **Rollback limits:** <What an application rollback does NOT undo (schema, backfilled
  or deleted data); the forward-fix or restore procedure instead.>

## Rollout and rollback

- **Rollout:** <Flags, staged exposure, preview checks, who approves.>
- **Rollback:** <Exact steps, the target it returns to, and what must be true first.>
