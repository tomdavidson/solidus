Smarter matrix would inlcude task to runner affinity by toolchain

# Get affected projects with their language
moon query projects --affected --json | jq '[.projects[] | {id, language}] | group_by(.language)'

Then your matrix becomes something like:

json
[
  {"toolchain": "rust", "projects": ["parser", "wasm-bindings"]},
  {"toolchain": "javascript", "projects": ["web", "docs"]}
]

Each shard runs moon ci scoped to its projects, so cargo only compiles once on the Rust runner and node_modules only installs on the JS runner.

Moon's query commands give you a lot to work with:

    moon query projects --affected --json for affected projects with metadata

    moon query tasks --affected --json for affected tasks

    moon project-graph --json for the full dependency graph

You could even get fancier and look at the dependency graph to keep projects that depend on each other on the same runner. But grouping by language/toolchain gets you 90% of the benefit with minimal complexity. Worth prototyping once you have more than one project type in the repo.