#!/usr/bin/env python3
"""Observed Claude plan quota utilization as a native Sidescope v1.1 answer.

Runs exactly one `agent-activity quota series` query (no shell, no other
query; it reads the retained journal only) or, with --convert-stdin, converts
one producer answer read from standard input and runs nothing. Prints one
`sidescope-chart-v1.1` JSON object holding at most one series: the readings of
one account and one quota window as a sampled gauge.

A reading is an instant, not an interval: interval `i` starts at reading `i`
and its end only orders readings. Instants keep RFC 3339 nanoseconds exactly
(integer seconds plus integer nanoseconds; no datetime, no float). Percent
values keep their exact decimal text. The bounded-read and decimal helpers are
duplicated in quota-estimate.py on purpose: each file is copied alone.

Exit 0 success, 1 refusal or producer failure (nothing printed), 2 usage.
"""
import argparse
import decimal
import json
import os
import re
import subprocess
import sys
import unicodedata

MAX_BYTES = 1048576
MAX_INTERVALS = 2048
MAX_NOTICES = 32
MAX_TEXT = 512
MAX_INTEGER_DIGITS = 24
MAX_FRACTION_DIGITS = 18
METRIC = "observed_percent"
WINDOWS = ("five_hour", "seven_day")
EDGE_KINDS = ("availability_changed", "limit_changed", "window_added", "window_removed")
NANOS = 1000000000

INSTANT = re.compile(
    r"\A(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?"
    r"([Zz]|[+-]\d{2}:\d{2})\Z"
)


class Refusal(Exception):
    """A refusal or producer failure; the text carries no data values."""


class Usage(Exception):
    pass


class QuietParser(argparse.ArgumentParser):
    def error(self, message):
        raise Usage("usage error")


def instant(text):
    """(epoch seconds, nanoseconds) of an RFC 3339 instant, integers only."""
    match = INSTANT.match(text) if isinstance(text, str) else None
    if match is None:
        raise Refusal("an instant is not RFC 3339")
    year, month, day, hour, minute, second = (int(match.group(i)) for i in range(1, 7))
    leap = year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)
    month_days = (31, 29 if leap else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
    if not (1 <= month <= 12 and 1 <= day <= month_days[month - 1]):
        raise Refusal("an instant is not a calendar date")
    if hour > 23 or minute > 59 or second > 59:
        raise Refusal("an instant is not a clock time")
    nanos = int((match.group(7) or "").ljust(9, "0"))
    zone = match.group(8)
    offset = 0
    if zone not in ("Z", "z"):
        zone_hours, zone_minutes = int(zone[1:3]), int(zone[4:6])
        if zone_hours > 23 or zone_minutes > 59:
            raise Refusal("an instant has an invalid offset")
        offset = (zone_hours * 3600 + zone_minutes * 60) * (1 if zone[0] == "+" else -1)
    shifted_year = year - (month <= 2)
    era = shifted_year // 400
    year_of_era = shifted_year - era * 400
    day_of_year = (153 * (month + (-3 if month > 2 else 9)) + 2) // 5 + day - 1
    day_of_era = year_of_era * 365 + year_of_era // 4 - year_of_era // 100 + day_of_year
    days = era * 146097 + day_of_era - 719468
    return (days * 86400 + hour * 3600 + minute * 60 + second - offset, nanos)


def check_text(text, limit=MAX_TEXT):
    if (
        not isinstance(text, str)
        or len(text) > limit
        or any(unicodedata.category(c) == "Cc" for c in text)
    ):
        raise Refusal("text is too long or has control characters")
    return text


def plain_decimal(value):
    """Exact plain notation of a Decimal, trimmed by string, within 24+18 digits.

    No arithmetic, normalize() or quantize(): the default context would round.
    """
    if not isinstance(value, decimal.Decimal) or not value.is_finite():
        raise Refusal("a number is not a finite decimal")
    sign, digit_tuple, exponent = value.as_tuple()
    digits = "".join(str(d) for d in digit_tuple).lstrip("0")
    stripped = digits.rstrip("0")
    exponent += len(digits) - len(stripped)
    digits = stripped
    if not digits:
        return "0"
    if exponent >= 0:
        if len(digits) + exponent > MAX_INTEGER_DIGITS:
            raise Refusal("a number has too many integer digits")
        text = digits + "0" * exponent
    else:
        fraction = -exponent
        if fraction > MAX_FRACTION_DIGITS:
            raise Refusal("a number has too many fraction digits")
        if len(digits) > fraction:
            whole, part = digits[:-fraction], digits[-fraction:]
        else:
            whole, part = "0", "0" * (fraction - len(digits)) + digits
        if len(whole) > MAX_INTEGER_DIGITS:
            raise Refusal("a number has too many integer digits")
        text = whole + "." + part
    return ("-" if sign else "") + text


def read_bounded(stream):
    data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise Refusal("input is over the 1 MiB bound")
    return data


def run_producer(argv):
    """One child, no shell, same process group, stdin null, stderr inherited."""
    try:
        child = subprocess.Popen(
            argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, shell=False
        )
    except OSError:
        raise Refusal("the producer could not be started")
    chunks = []
    total = 0
    try:
        while True:
            chunk = os.read(child.stdout.fileno(), 65536)
            if not chunk:
                break
            total += len(chunk)
            if total > MAX_BYTES:
                child.kill()
                raise Refusal("producer output is over the 1 MiB bound")
            chunks.append(chunk)
        status = child.wait()
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
        child.stdout.close()
    if status != 0:
        raise Refusal("the producer did not exit normally")
    return b"".join(chunks)


def reject_duplicates(pairs):
    keys = [key for key, _ in pairs]
    if len(set(keys)) != len(keys):
        raise Refusal("an object repeats a key")
    return dict(pairs)


def refuse_constant(_name):
    raise Refusal("a number is not a finite decimal")


def decode(data):
    try:
        return json.loads(
            data.decode("utf-8"),
            parse_float=decimal.Decimal,
            parse_int=decimal.Decimal,
            parse_constant=refuse_constant,
            object_pairs_hook=reject_duplicates,
        )
    except (UnicodeDecodeError, ValueError, RecursionError):
        raise Refusal("input is not valid JSON")


def obj(value):
    if not isinstance(value, dict):
        raise Refusal("an object was expected")
    return value


def arr(value):
    if not isinstance(value, list):
        raise Refusal("an array was expected")
    return value


def format_instant(point):
    """UTC `Z` text of (seconds, nanoseconds), nanoseconds trimmed of zeros."""
    seconds, nanos = point
    days, rest = divmod(seconds, 86400)
    shifted = days + 719468
    era = shifted // 146097
    day_of_era = shifted - era * 146097
    year_of_era = (
        day_of_era - day_of_era // 1460 + day_of_era // 36524 - day_of_era // 146096
    ) // 365
    day_of_year = day_of_era - (365 * year_of_era + year_of_era // 4 - year_of_era // 100)
    month_part = (5 * day_of_year + 2) // 153
    day = day_of_year - (153 * month_part + 2) // 5 + 1
    month = month_part + (3 if month_part < 10 else -9)
    year = year_of_era + era * 400 + (month <= 2)
    text = "%04d-%02d-%02dT%02d:%02d:%02d" % (
        year, month, day, rest // 3600, rest % 3600 // 60, rest % 60,
    )
    if nanos:
        text += "." + ("%09d" % nanos).rstrip("0")
    return text + "Z"


def total_nanos(point):
    return point[0] * NANOS + point[1]


def percent_cell(value):
    if value is None:
        return None
    if not isinstance(value, decimal.Decimal) or value < 0:
        raise Refusal("a percent is neither null nor a non-negative number")
    return plain_decimal(value)


def select_account(accounts, args):
    field, wanted = ("accountKey", args.account_key)
    if args.account_label is not None:
        field, wanted = ("accountLabel", args.account_label)
    matches = [a for a in accounts if obj(a).get(field) == wanted]
    if not matches:
        raise Refusal("the selector matches no account")
    if len(matches) > 1:
        raise Refusal("the selector matches more than one account")
    return matches[0]


def window_series(account, args, start, end):
    """(series, intervals, notices) for one selected account and window."""
    key = account.get("accountKey")
    if not isinstance(key, str) or not key:
        raise Refusal("the account has no key")
    check_text(key)
    readings = arr(account.get("at"))
    if len(readings) > MAX_INTERVALS:
        raise Refusal("more than 2048 readings")
    points = []
    for text in readings:
        point = instant(text)
        if point < start or point >= end:
            raise Refusal("a reading lies outside the range")
        if points and point <= points[-1]:
            raise Refusal("readings are duplicated or not increasing")
        points.append(point)
    windows = [w for w in arr(account.get("windows")) if obj(w).get("id") == args.window]
    if len(windows) > 1:
        raise Refusal("a window is reported twice")
    if not windows:
        return None, [], ["The account reported no readings for window %s." % args.window]
    window = windows[0]
    percents = arr(window.get("percent"))
    if len(percents) != len(points):
        raise Refusal("percent does not align with the readings")
    values = []
    quality = []
    for percent in percents:
        cell = percent_cell(percent)
        values.append(cell)
        quality.append("missing" if cell is None else "complete")
    resets = arr(window.get("resets", []))
    previous = -1
    reset_indexes = []
    for reset in resets:
        reset = obj(reset)
        index = reset.get("index")
        if (
            not isinstance(index, decimal.Decimal)
            or index != index.to_integral_value()
            or not 0 <= index < len(points)
            or index <= previous
        ):
            raise Refusal("a reset index is out of range or not increasing")
        previous = int(index)
        reset_indexes.append(previous)
        if reset.get("resetsAt") is not None:
            instant(reset.get("resetsAt"))
    breaks = set(reset_indexes[1:])
    gap_breaks = 0
    limit = args.max_gap_minutes * 60 * NANOS
    for index in range(1, len(points)):
        if total_nanos(points[index]) - total_nanos(points[index - 1]) > limit:
            gap_breaks += 1
            breaks.add(index)
    edge_counts = dict((kind, 0) for kind in EDGE_KINDS)
    for edge in arr(window.get("edges", [])):
        kind = obj(edge).get("kind")
        if kind in edge_counts:
            edge_counts[kind] += 1
    notices = []
    intervals = []
    for index, point in enumerate(points):
        follow = points[index + 1] if index + 1 < len(points) else end
        intervals.append({"start": format_instant(point), "end": format_instant(follow)})
    if len(reset_indexes) > 1:
        notices.append(
            "%d reset-time change(s) in this range; the line is broken there and no change"
            " is computed across one." % (len(reset_indexes) - 1)
        )
    for kind in EDGE_KINDS:
        if edge_counts[kind]:
            notices.append("%d recorded %s edge(s) in this range." % (edge_counts[kind], kind))
    if gap_breaks:
        notices.append(
            "%d gap break(s): readings more than %d minutes apart are not joined; that"
            " spacing rule is this wrapper's setting, not producer data."
            % (gap_breaks, args.max_gap_minutes)
        )
    series = {
        "id": check_text(
            json.dumps([key, args.window], ensure_ascii=False, separators=(",", ":"))
        ),
        "dimensions": {},
        "metrics": {
            METRIC: {"values": values, "quality": quality, "breaks": sorted(breaks)}
        },
    }
    return series, intervals, notices


def convert(answer, args):
    answer = obj(answer)
    generated_at = check_text(answer.get("generatedAt"))
    instant(generated_at)
    truncated = answer.get("truncated", False)
    if not isinstance(truncated, bool):
        raise Refusal("truncated is not a boolean")
    start, end = instant(args.from_), instant(args.to)
    accounts = arr(answer.get("accounts"))
    series, intervals, notices = [], [], []
    if accounts:
        found = select_account(accounts, args)
        one, intervals, notices = window_series(found, args, start, end)
        if one is not None:
            series.append(one)
    if truncated:
        notices.insert(
            0,
            "Earlier readings exist but were not returned; the journal holds more than was"
            " read.",
        )
    if len(notices) > MAX_NOTICES:
        raise Refusal("too many notices")
    for notice in notices:
        check_text(notice)
    result = {
        "version": "sidescope-chart-v1.1",
        "generated_at": generated_at,
        "range": {"start": args.from_, "end": args.to, "timezone": args.timezone},
        "intervals": intervals,
        "metrics": {
            METRIC: {
                "label": "Observed quota utilization",
                "unit": "percent",
                "quantity": "gauge",
                "estimate": False,
                "sampled": True,
            }
        },
        "series": series,
        "notices": notices,
    }
    return result


def parse_args(argv):
    parser = QuietParser(add_help=False, allow_abbrev=False)
    parser.add_argument("--account-key", dest="account_key")
    parser.add_argument("--account-label", dest="account_label")
    parser.add_argument("--window", required=True, choices=WINDOWS)
    parser.add_argument("--max-gap-minutes", dest="max_gap_minutes", default="45")
    parser.add_argument("--from", dest="from_", required=True)
    parser.add_argument("--to", required=True)
    parser.add_argument("--timezone", required=True)
    parser.add_argument("--producer", default="agent-activity")
    parser.add_argument("--convert-stdin", dest="convert_stdin", action="store_true")
    try:
        args = parser.parse_args(argv)
        if (args.account_key is None) == (args.account_label is None):
            raise Usage("usage error")
        if not (args.account_key if args.account_key is not None else args.account_label):
            raise Usage("usage error")
        if not re.fullmatch(r"[0-9]{1,4}", args.max_gap_minutes):
            raise Usage("usage error")
        args.max_gap_minutes = int(args.max_gap_minutes)
        if not 1 <= args.max_gap_minutes <= 1440:
            raise Usage("usage error")
        if not args.timezone or args.timezone != args.timezone.strip():
            raise Usage("usage error")
        check_text(args.timezone, 64)
        if not args.producer:
            raise Usage("usage error")
        if instant(args.to) <= instant(args.from_):
            raise Usage("usage error")
    except Refusal:
        raise Usage("usage error")
    return args


def main(argv):
    try:
        args = parse_args(argv)
    except Usage:
        sys.stderr.write("quota-observed: usage error\n")
        return 2
    try:
        if args.convert_stdin:
            data = read_bounded(sys.stdin.buffer)
        else:
            data = run_producer(
                [
                    args.producer, "quota", "series", "--format", "json",
                    "--from", args.from_, "--to", args.to, "--window", args.window,
                ]
            )
        result = convert(decode(data), args)
        out = (
            json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        if len(out) > MAX_BYTES:
            raise Refusal("output is over the 1 MiB bound")
    except Refusal as refusal:
        sys.stderr.write("quota-observed: refused: %s\n" % refusal)
        return 1
    except (UnicodeError, RecursionError, MemoryError, ArithmeticError, OSError):
        sys.stderr.write("quota-observed: refused: input could not be converted\n")
        return 1
    sys.stdout.buffer.write(out)
    sys.stdout.buffer.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
