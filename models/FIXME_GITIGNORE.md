`.gitignore:48` has a blanket `models/` rule meant to keep large binary artifacts
(`dagda-0.2.bin`, `dagda-0.2.vec`) out of git, but it also swallows the `Modelfile.*`
files, which are small text configs that should be tracked. They only made it into
history because they were force-added before the ignore rule existed.

Fix: move `Modelfile.qwen-clara` / `Modelfile.qwen-clara-27b` out of `models/` (or
add a `!models/Modelfile.*` negation to `.gitignore`) so future edits don't need
`git add -f` and don't risk silently going untracked.

Delete this file once fixed.
