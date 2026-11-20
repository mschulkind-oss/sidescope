#!/usr/bin/env python3
"""Estimated Claude plan quota contribution as a native Sidescope answer.

Runs exactly one `agent-activity ledger series` query (no shell, no other
query) or, with --convert-stdin, converts one producer answer read from
standard input and runs nothing. Prints one `sidescope-chart-v1` JSON object.

Estimated points are fitted, not the provider's bar; the unit is
"percentage points" so a bounded-percent card refuses them. Every number is
read from its original decimal text and never passes through a binary float.
The bounded-read and decimal helpers are duplicated in quota-observed.py on
purpose: each file is copied alone.

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
MAX_SERIES = 256
MAX_CELLS = 100000
MAX_NOTICES = 32
MAX_TEXT = 512
MAX_INTEGER_DIGITS = 24
MAX_FRACTION_DIGITS = 18

BARS = {
    "weekly": {
        "percent": "subscriptionWeeklyPercent",
        "unweighted": "subscriptionWeeklyUnweighted",
        "weight": "weekly",
        "metric": "weekly_quota_points",
        "label": "Estimated weekly quota contribution",
        "words": "weekly",
    },
    "five-hour": {
        "percent": "subscriptionFiveHourPercent",
        "unweighted": "subscriptionFiveHourUnweighted",
        "weight": "fiveHour",
        "metric": "five_hour_quota_points",
        "label": "Estimated five-hour quota contribution",
        "words": "five-hour",
    },
}
BUCKETS = ("minute", "5min", "15min", "hour", "day")
GROUPS = ("none", "model", "project")
PRODUCER_GROUP = {"none": "none", "model": "model", "project": "workspace"}

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


def count_cell(value):
    """A non-negative integer count (a Decimal with no fractional part) or None."""
    if value is None:
        return None
    if not isinstance(value, decimal.Decimal) or not value.is_finite() or value < 0:
        raise Refusal("a count is not a non-negative integer")
    if value != value.to_integral_value():
        raise Refusal("a count is not a non-negative integer")
    return value


def percent_cell(value):
    if value is None:
        return None
    if not isinstance(value, decimal.Decimal) or value < 0:
        raise Refusal("a percent column holds a bad number")
    return plain_decimal(value)


def notice_texts(answer, args, groups):
    weights = answer.get("quotaWeights")
    if not isinstance(weights, dict):
        raise Refusal("the answer has no quotaWeights")
    bar = BARS[args.quota]

    def shown(key):
        value = weights.get(key)
        if value is None:
            return "unknown"
        if not isinstance(value, str):
            raise Refusal("a quotaWeights date is not text")
        return value

    price = weights.get("seatPriceUSDPerMonth")
    price_text = "unknown" if price is None else plain_decimal(price)
    notices = [
        "Quota weights generated %s; fitted from %s through %s."
        % (shown("generated"), shown("fittedFrom"), shown("fittedThrough")),
        "Weights come from a plan priced at %s USD per month." % price_text,
    ]
    if args.group_by == "model":
        per_model = weights.get("percentPerUSD", {})
        obj(per_model)
        missing = 0
        for group in groups:
            entry = per_model.get(group["keys"][0])
            if entry is not None:
                obj(entry)
            if entry is None or entry.get(bar["weight"]) is None:
                missing += 1
        if missing:
            notices.append(
                "%d grouped model(s) have no fitted %s weight; their usage is a lower bound."
                % (missing, bar["words"])
            )
    notices.append(
        "Estimated points are fitted, not published by the provider, and exclude resets"
        " and other clients."
    )
    return notices


def convert(answer, args):
    answer = obj(answer)
    bar = BARS[args.quota]
    generated_at = check_text(answer.get("generatedAt"))
    instant(generated_at)
    range_ = obj(answer.get("range"))
    start = check_text(range_.get("from"))
    end = check_text(range_.get("to"))
    if instant(end) <= instant(start):
        raise Refusal("the range is not positive")
    buckets = arr(answer.get("buckets"))
    if len(buckets) > MAX_INTERVALS:
        raise Refusal("more than 2048 buckets")
    intervals = []
    previous_end = None
    for bucket in buckets:
        bucket = obj(bucket)
        b_start, b_end = check_text(bucket.get("start")), check_text(bucket.get("end"))
        if instant(b_end) <= instant(b_start):
            raise Refusal("a bucket is not positive")
        if previous_end is not None and instant(b_start) < previous_end:
            raise Refusal("buckets overlap or are out of order")
        previous_end = instant(b_end)
        intervals.append({"start": b_start, "end": b_end})
    groups = arr(answer.get("groups"))
    if len(groups) > MAX_SERIES:
        raise Refusal("more than 256 series")
    if len(groups) * len(intervals) > MAX_CELLS:
        raise Refusal("more than 100000 cells")
    wanted_keys = 0 if args.group_by == "none" else 1
    dimension = {"none": None, "model": "model", "project": "project"}[args.group_by]
    series = []
    seen = set()
    for group in groups:
        group = obj(group)
        keys = arr(group.get("keys"))
        if len(keys) != wanted_keys:
            raise Refusal("a group does not have the requested keys")
        for key in keys:
            check_text(key)
        columns = obj(group.get("columns"))
        percents = columns.get(bar["percent"])
        counts = columns.get(bar["unweighted"])
        for column in (percents, counts):
            if column is not None and len(arr(column)) != len(intervals):
                raise Refusal("a column does not align with the buckets")
        values, quality = [], []
        for index in range(len(intervals)):
            percent = "0" if percents is None else percent_cell(percents[index])
            count = 0 if counts is None else count_cell(counts[index])
            if percent is None or count is None:
                values.append(None)
                quality.append("missing")
            else:
                values.append(percent)
                quality.append("partial" if count > 0 else "complete")
        dimensions = {}
        if dimension is not None:
            dimensions[dimension] = "" if keys[0] == "(unknown)" else keys[0]
        series_id = check_text(json.dumps(keys, ensure_ascii=False, separators=(",", ":")))
        shown_key = (series_id, tuple(sorted(dimensions.items())))
        if series_id in seen or shown_key[1] in seen:
            raise Refusal("two groups share one key")
        seen.add(series_id)
        seen.add(shown_key[1])
        series.append(
            {
                "id": series_id,
                "dimensions": dimensions,
                "metrics": {bar["metric"]: {"values": values, "quality": quality}},
            }
        )
    notices = notice_texts(answer, args, groups)
    if len(notices) > MAX_NOTICES:
        raise Refusal("too many notices")
    for notice in notices:
        check_text(notice)
    result = {
        "version": "sidescope-chart-v1",
        "generated_at": generated_at,
        "range": {"start": start, "end": end, "timezone": args.timezone},
        "intervals": intervals,
        "metrics": {
            bar["metric"]: {
                "label": bar["label"],
                "unit": "percentage points",
                "quantity": "interval_total",
                "estimate": True,
                "unknown_contribution": "nonnegative",
            }
        },
        "series": series,
        "notices": notices,
    }
    generation = answer.get("generation")
    if generation is not None:
        if not isinstance(generation, decimal.Decimal) or generation != generation.to_integral_value():
            raise Refusal("generation is not an integer")
        result["revision"] = plain_decimal(generation)
    return result


def parse_args(argv):
    parser = QuietParser(add_help=False, allow_abbrev=False)
    parser.add_argument("--quota", required=True, choices=sorted(BARS))
    parser.add_argument("--bucket", required=True, choices=BUCKETS)
    parser.add_argument("--group-by", dest="group_by", required=True, choices=GROUPS)
    parser.add_argument("--from", dest="from_", required=True)
    parser.add_argument("--to", required=True)
    parser.add_argument("--timezone", required=True)
    parser.add_argument("--producer", default="agent-activity")
    parser.add_argument("--convert-stdin", dest="convert_stdin", action="store_true")
    try:
        args = parser.parse_args(argv)
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
        sys.stderr.write("quota-estimate: usage error\n")
        return 2
    try:
        if args.convert_stdin:
            data = read_bounded(sys.stdin.buffer)
        else:
            bar = BARS[args.quota]
            data = run_producer(
                [
                    args.producer, "ledger", "series", "--format", "json",
                    "--layout", "columns", "--bucket", args.bucket,
                    "--group-by", PRODUCER_GROUP[args.group_by],
                    "--channel", "subscription",
                    "--columns", "%s,%s" % (bar["percent"], bar["unweighted"]),
                    "--from", args.from_, "--to", args.to,
                    "--timezone", args.timezone,
                ]
            )
        result = convert(decode(data), args)
        out = (
            json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        if len(out) > MAX_BYTES:
            raise Refusal("output is over the 1 MiB bound")
    except Refusal as refusal:
        sys.stderr.write("quota-estimate: refused: %s\n" % refusal)
        return 1
    except (UnicodeError, RecursionError, MemoryError, ArithmeticError, OSError):
        sys.stderr.write("quota-estimate: refused: input could not be converted\n")
        return 1
    sys.stdout.buffer.write(out)
    sys.stdout.buffer.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
