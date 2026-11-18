#!/usr/bin/env bash
# Sourced by visual-test.sh inside its private compositor/session.
run_density_suite() {
  local scenario theme density prefix log x y width height
  for scenario in ${SIDESCOPE_DENSITY_SCENARIOS:-screen-fit fit-controls fit-theme fit-details fit-info fit-report-details fit-report-info fit-missing fit-extreme fit-errors fit-high fit-sensors-zero fit-sensors-missing fit-sensors-error fit-stale-quota}; do
    for theme in original paper terminal; do
      # Controls/details need only one representative run; geometry covers all three.
      if [[ "$scenario" != screen-fit && "$theme" != original ]]; then continue; fi
      kill "$app_pid"; wait "$app_pid" || :; app_pid=
      swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
      mkdir -p "$XDG_CONFIG_HOME/sidescope"
      printf 'density = "screen-fit"\ntheme = "%s"\nshow_agy_gpt_oss = true\n' "$theme" > "$XDG_CONFIG_HOME/sidescope/config.toml"
      cp "$XDG_CONFIG_HOME/sidescope/config.toml" "$RUNTIME/density-before.toml"
      log="$RUNTIME/app.log"
      prefix="${CAPTURE%.png}-density-$scenario-$theme"
      SIDESCOPE_VISUAL_SCENARIO="$scenario" "$BIN" --report "$ROOT/tests/fixtures/screen-fit-usage.json" > "$log" 2>&1 & app_pid=$!
      for _ in {1..100}; do
        if swaymsg -t get_tree | jq -e '[.. | objects | select(.name? == "Sidescope")] | length == 1' >/dev/null; then break; fi
        kill -0 "$app_pid" || { cat "$log" >&2; return 1; }
        sleep 0.1
      done
      swaymsg '[title="Sidescope"] focus' >/dev/null
      swaymsg 'resize set width 640 px' >/dev/null
      sleep 2
      read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
      grim -g "$x,$y ${width}x${height}" "$prefix-wide.png"
      if [[ "$scenario" == screen-fit && "$theme" == original ]]; then
        # Curated themes stay a single 21-choice registry, not density variants.
        for preview in original paper terminal blueprint editorial contrast retro cloud minimal nord solar rose ocean ember orchid forest cobalt sand mint noir arcade; do
          for _ in {1..40}; do
            kill -0 "$app_pid" || { cat "$log" >&2; return 1; }
            if grep -Eq "DENSITY_GEOMETRY mode=ScreenFit theme=$preview .*no_scroll=true cards=13 horizontal_clip=false" "$log"; then break; fi
            sleep 0.1
          done
          grep -Eq "DENSITY_GEOMETRY mode=ScreenFit theme=$preview .*no_scroll=true cards=13 horizontal_clip=false" "$log"
          grim -g "$x,$y ${width}x${height}" "${CAPTURE%.png}-density-theme-$preview.png"
          wtype ']'
        done
        sleep 0.85
        # Actual height, not a 1440px assumption, selects the tighter four-cell CPU.
        swaymsg 'output HEADLESS-1 mode 2560x1360' >/dev/null
        sleep 1.5
        grep -q 'DENSITY_GEOMETRY mode=ScreenFit theme=original width=636 height=1333 .*cards=13 horizontal_clip=false' "$log"
        # The available height is a measurement of the header and footer, so it
        # moves with any text-metric or spacing change (1157 under the hinted
        # font metrics the app asks for). What this step is about is which CPU
        # plan the 1333px window height selects; whether that plan leaves the
        # thirteen-card fixture scrolling is itself a content-height measurement,
        # and the 873px step below is where genuine overflow is asserted.
        grep -q 'FIT_PLAN width=612 available=[0-9]* .*core_columns=4' "$log"
        grim -g "$x,$y ${width}x1360" "${CAPTURE%.png}-density-short-height.png"
        swaymsg 'output HEADLESS-1 mode 2560x900' >/dev/null
        sleep 1.5
        grep -q 'DENSITY_GEOMETRY mode=ScreenFit theme=original width=636 height=873 .*no_scroll=false' "$log"
        grim -g "$x,$y ${width}x900" "${CAPTURE%.png}-density-short-overflow.png"
        swaymsg 'output HEADLESS-1 mode 2560x1440' >/dev/null
        sleep 1.5
      fi
      if [[ "$scenario" == fit-theme ]]; then
        sleep 5
        grep -q 'THEME_LIVE_UPDATE tick=3 selected=blueprint local_and_report_preserved=true' "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-after-updates.png"
      fi
      if [[ "$scenario" == fit-report-details || "$scenario" == fit-report-info ]]; then
        sleep 1.4
        grep -q 'REPORT_TRANSITION tick=1 .*visible=true focus_preserved=true hosts_preserved=true packing_preserved=true' "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-failure.png"
        sleep 2
        grep -q 'REPORT_TRANSITION tick=2 .*visible=true focus_preserved=true hosts_preserved=true packing_preserved=true' "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-changed-failure.png"
        sleep 2
        grep -q 'REPORT_TRANSITION tick=3 .*visible=true focus_preserved=true hosts_preserved=true packing_preserved=true' "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-recovery.png"
        wtype -k Escape
      fi
      if [[ "$scenario" == fit-stale-quota ]]; then
        # A stale quota reading keeps the window's time mark and loses only the
        # verdict it can no longer support; a current reading keeps both.
        grep -q 'TIME_MARK card=anthropic metric=0 elapsed=25 verdict=none' "$log"
        grep -q 'TIME_MARK card=anthropic metric=1 elapsed=58 verdict=none' "$log"
        grep -q 'TIME_MARK card=openai metric=1 elapsed=55 verdict=pace-even' "$log"
        printf 'Stale quota time marks verified: %s\n' "$prefix"
      fi
      if [[ "$scenario" == fit-details || "$scenario" == fit-info ]]; then
        # Updates tick every SAMPLE_SECS from a delayed start, so tick 3 lands a
        # little past a fixed sleep. Poll instead of guessing the interval.
        for _ in {1..60}; do
          if grep -q 'LIVE_DETAILS_UPDATE tick=3 visible=true focus_preserved=true' "$log"; then break; fi
          sleep 0.25
        done
        grep -q 'LIVE_DETAILS_UPDATE tick=3 visible=true focus_preserved=true' "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-details.png"
        wtype -k Escape
      fi
      if [[ "$scenario" == screen-fit || "$scenario" == fit-controls ]]; then
        grep -Eq "DENSITY_GEOMETRY mode=ScreenFit theme=$theme .*no_scroll=true cards=13 horizontal_clip=false" "$log"
        wtype -d 100 'abcd'
        sleep 1.5
        grep -q "DENSITY_GEOMETRY mode=Comfortable theme=$theme" "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-comfortable.png"
        wtype -d 100 'd]'
        sleep 1.5
        grep -Eq 'DENSITY_GEOMETRY mode=ScreenFit .*no_scroll=true' "$log"
        grim -g "$x,$y ${width}x${height}" "$prefix-next-theme.png"
      fi
      swaymsg 'output HEADLESS-1 mode 1280x900' >/dev/null
      swaymsg 'resize set width 378 px' >/dev/null
      sleep 1.5
      read -r x y width height < <(swaymsg -t get_tree | jq -r '.. | objects | select(.name? == "Sidescope") | [.rect.x,.rect.y,.rect.width,.rect.height] | @tsv' | head -1)
      grim -g "$x,$y ${width}x${height}" "$prefix-narrow.png"
      if [[ "$scenario" == fit-info ]]; then
        grep -q 'NARROW_INFO_GEOMETRY width=' "$log"
        wtype -k Escape
      fi
      wtype -d 30 -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down -k Page_Down
      sleep 0.4
      grim -g "$x,$y ${width}x${height}" "$prefix-narrow-bottom.png"
      grep -q 'DENSITY_GEOMETRY mode=ScreenFit .*width=374 .*no_scroll=false' "$log"
      cmp "$RUNTIME/density-before.toml" "$XDG_CONFIG_HOME/sidescope/config.toml"
      cp "$log" "$prefix-geometry.log"
      kill -0 "$app_pid" || { cat "$log" >&2; return 1; }
      if grep -Ei 'panicked|CRITICAL|Gtk-WARNING|Adwaita-WARNING' "$log"; then cat "$log" >&2; return 1; fi
      printf 'Density fixture verified: %s\n' "$prefix"
    done
  done
}
run_density_suite
