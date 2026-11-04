# Follow herdr-app's focus through its view file

Under Herdr, the review pane prompts the agent of its tab the user focused last, which Herdr's
`pane.focused` events tell. herdr-core keeps no focus: herdr-app keeps it on each of its
screens, writes it to its home's `view.json` by its own pane IDs, and keeps its layout, which
says the terminal each pane shows, in herdr-core's `herdr-app` document. A mod sees the focus
with `focused()`, but a copy of a mod lives only while it shows, and the launcher's closes at
once.

The review pane follows `view.json` (`crates/herdr-app-focus`): inotify on the home tells it
each time herdr-app writes the file, and it maps the focused pane of the selected tab of the
selected space to its terminal through the layout, which it reads again on each `doc` event of
herdr-core. It tells each focused terminal as Herdr's `pane.focused` does, so the agent target
works as under Herdr. The mod passes its window's home (`appHome()`) with `--herdr-app-home`:
the pane's terminal gets herdr-core's environment, whose `HERDR_APP_HOME`, if any, is the one
herdr-core started with. Without the option, the pane follows `~/.config/herdr-app`.

Considered:
- The launcher mod publishes `focused()` as a label of the review pane's terminal: herdr-core
  would carry the focus as an event, but only from a copy of the mod that stays, a pane or a
  kept window in the user's layout, for as long as any review pane runs.
- Ask herdr-app to put the focus in the core: the right home for it in the long run, but out of
  the reviewer's hands. `view.json` is herdr-app's own file, with no promise of its format;
  when herdr-app changes it, or puts focus in the core, only `crates/herdr-app-focus` changes.
