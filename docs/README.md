# Kandado docs

Three sets of docs, each with one job. They link to each other but never repeat each other.

| Folder | Answers | Source of truth for |
| --- | --- | --- |
| [design-system/](design-system/project/README.md) | What does Kandado look like and say? | Tokens, components, icons, the desktop and extension screens and their IDs, copy rules |
| [architecture/](architecture/README.md) | How is Kandado built? | Repository layout, crates, key hierarchy, API, database schema, client structure |
| [implementation-plan/](implementation-plan/README.md) | In what order do we build it? | Phases, exit criteria, screen coverage, open decisions |

## Boundaries

- **design-system/** is a mirror of the Kandado Hardware design system artifact, the direction chosen in the Kandado Redesign. Change it through the artifact, not by hand here. It knows nothing about Rust, PostgreSQL or phases. Its [screens.md](design-system/project/screens.md) keeps the D- and E- screen IDs and says which Redesign board draws each screen. The previous design system (with the mobile M- screens) is in git history and in its own artifact.
- **architecture/** refers to screens only by ID (D-07, E-16). It never restates what a screen contains; follow the ID to [screens.md](design-system/project/screens.md).
- **implementation-plan/** refers to architecture by link and to screens by ID. It holds no schema, no API shapes and no visual rules.
- Source code never reads from `docs/`. Design tokens are copied into `src/` by a sync script and checked in CI (see [desktop.md](architecture/desktop.md#design-tokens)).

## Where to start

1. [implementation-plan/README.md](implementation-plan/README.md) for the phases.
2. [implementation-plan/decisions.md](implementation-plan/decisions.md) for what still needs a decision, and where the plan differs from the design.
3. [implementation-plan/screen-coverage.md](implementation-plan/screen-coverage.md) to see which phase builds each screen.
