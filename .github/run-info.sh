#!/usr/bin/env bash
# Source this file: source run-debug.sh
# Then use: timing [run-id], logs [run-id], or just run directly for both

_resolve_run_id() {
  local run_id="${1:-}"
  if [[ -z "$run_id" ]]; then
    run_id=$(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')
    echo "Using latest run: $run_id" >&2
  fi
  echo "$run_id"
}

timing() {
  local run_id
  run_id=$(_resolve_run_id "$1")

  echo "=== Workflow Run: $run_id ==="
  echo ""

  gh run view "$run_id" --json jobs --jq '
    .jobs | sort_by(.startedAt) | .[] |
    "── \(.name) (\(.conclusion // "running")) ──",
    "   Started:  \(.startedAt)",
    "   Duration: \(
      if .completedAt then
        ((.completedAt | fromdateiso8601) - (.startedAt | fromdateiso8601)) as $dur |
        "\($dur / 60 | floor)m \($dur % 60)s"
      else "in progress"
      end
    )",
    "",
    (.steps | map(
      (if .completedAt and .startedAt then
        ((.completedAt | fromdateiso8601) - (.startedAt | fromdateiso8601))
      else 0 end) as $dur |
      "   \(if $dur >= 60 then "!!" elif $dur >= 30 then ">>" else "  " end) \(
        if .completedAt and .startedAt then
          "\($dur / 60 | floor)m \($dur % 60 | tostring | if length < 2 then "0" + . else . end)s"
        else "--:--"
        end
      )  \(.conclusion // "---" | if . == "success" then "pass" elif . == "skipped" then "skip" elif . == "failure" then "FAIL" else . end)  \(.name)"
    ) | join("\n")),
    "",
    ""
  '
}

logs() {
  local run_id
  run_id=$(_resolve_run_id "$1")
  local outfile="run-${run_id}.log"

  echo "Downloading logs for run $run_id..." >&2

  gh run view "$run_id" --log > "$outfile" 2>&1

  if [[ $? -eq 0 ]]; then
    echo "Saved to $outfile ($(wc -l < "$outfile") lines)" >&2
    echo "" >&2
    echo "Quick search tips:" >&2
    echo "  grep -i error $outfile" >&2
    echo "  grep -i warn $outfile" >&2
    echo "  grep -iE 'fail|error|panic' $outfile" >&2
  else
    echo "Failed to download logs" >&2
    cat "$outfile" >&2
  fi
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  run_id=$(_resolve_run_id "$1")
  timing "$run_id"
  logs "$run_id"
fi