# Develop locally before the owner separates the repositories

Archodin and Basileia will ultimately live in separate repositories: Archodin is intended to be public and Basileia private. Separate repositories make the library/application ownership and publication boundaries explicit, rather than depending on selective publication from a mixed repository.

For now, development continues locally in this checkout. The owner will perform repository separation and pushes manually at the end of program development; agents must not create remotes, split repositories or publish code as part of this work. The target repository boundary must still be respected in local dependencies and assets so that Archodin can be extracted without Basileia-specific behavior.

This decision establishes the future separation, not a final crate layout or a claim that either product is complete. Existing files remain in place until an explicit migration plan is agreed.
