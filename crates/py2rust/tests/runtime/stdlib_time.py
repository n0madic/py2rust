# Test file for time module

import time

# ===========================================================
# Test wall-clock time APIs
# ===========================================================

t1: float = time.time()
t2: float = time.time()
assert t2 >= t1
assert t1 > 0.0
print("time() tests passed")

t_ns1: int = time.time_ns()
t_ns2: int = time.time_ns()
assert t_ns2 >= t_ns1
assert t_ns1 > 0
print("time_ns() tests passed")

# ===========================================================
# Test monotonic/perf/process clocks
# ===========================================================

mono1: float = time.monotonic()
time.sleep(0.01)
mono2: float = time.monotonic()
assert mono2 >= mono1
print("monotonic() tests passed")

mono_ns1: int = time.monotonic_ns()
time.sleep(0.005)
mono_ns2: int = time.monotonic_ns()
assert mono_ns2 >= mono_ns1
print("monotonic_ns() tests passed")

perf1: float = time.perf_counter()
time.sleep(0.005)
perf2: float = time.perf_counter()
assert perf2 >= perf1
print("perf_counter() tests passed")

perf_ns1: int = time.perf_counter_ns()
time.sleep(0.005)
perf_ns2: int = time.perf_counter_ns()
assert perf_ns2 >= perf_ns1
print("perf_counter_ns() tests passed")

proc1: float = time.process_time()
counter: int = 0
for i in range(20000):
    counter = counter + i
proc2: float = time.process_time()
assert proc2 >= proc1
print("process_time() tests passed")

proc_ns1: int = time.process_time_ns()
for i in range(20000):
    counter = counter + i
proc_ns2: int = time.process_time_ns()
assert proc_ns2 >= proc_ns1
print("process_time_ns() tests passed")

# ===========================================================
# Test sleep signatures and from-imports
# ===========================================================

time.sleep(0.0)
time.sleep(0.001)
time.sleep(1)

from time import sleep, time as now, monotonic, perf_counter_ns, process_time_ns

start_from_import: float = monotonic()
sleep(0.005)
end_from_import: float = monotonic()
assert end_from_import >= start_from_import

now_value: float = now()
assert now_value > 0.0

perf_ns_from_1: int = perf_counter_ns()
sleep(0.001)
perf_ns_from_2: int = perf_counter_ns()
assert perf_ns_from_2 >= perf_ns_from_1

proc_ns_from_1: int = process_time_ns()
for i in range(10000):
    counter = counter + i
proc_ns_from_2: int = process_time_ns()
assert proc_ns_from_2 >= proc_ns_from_1

print("from-import tests passed")

# ===========================================================
# Test localtime/gmtime tuple APIs
# ===========================================================

epoch_gm = time.gmtime(0.0)
assert epoch_gm[0] == 1970
assert epoch_gm[1] == 1
assert epoch_gm[2] == 1
assert epoch_gm[3] == 0
assert epoch_gm[4] == 0
assert epoch_gm[5] == 0
assert epoch_gm[6] == 3
assert epoch_gm[7] == 1
assert epoch_gm[8] == 0

epoch_local = time.localtime(0.0)
assert epoch_local[0] > 0
assert 1 <= epoch_local[1] <= 12
assert 1 <= epoch_local[2] <= 31
assert 0 <= epoch_local[3] <= 23
assert 0 <= epoch_local[4] <= 59
assert 0 <= epoch_local[5] <= 59
assert 0 <= epoch_local[6] <= 6
assert 1 <= epoch_local[7] <= 366
assert epoch_local[8] == 0 or epoch_local[8] == 1

now_gm = time.gmtime()
now_local = time.localtime()
assert now_gm[0] > 0
assert now_local[0] > 0

before_epoch = time.gmtime(-1.0)
assert before_epoch[0] == 1969
assert before_epoch[1] == 12
assert before_epoch[2] == 31
assert before_epoch[3] == 23
assert before_epoch[4] == 59
assert before_epoch[5] == 59
assert before_epoch[6] == 2
assert before_epoch[7] == 365

plus_one = time.gmtime(1)
assert plus_one[0] == 1970
assert plus_one[1] == 1
assert plus_one[2] == 1
assert plus_one[5] == 1
before_epoch_local = time.localtime(-1.0)
assert before_epoch_local[0] > 0
assert 1 <= before_epoch_local[1] <= 12
assert 1 <= before_epoch_local[2] <= 31
assert 0 <= before_epoch_local[3] <= 23
assert 0 <= before_epoch_local[4] <= 59
assert 0 <= before_epoch_local[5] <= 59
assert 0 <= before_epoch_local[6] <= 6
assert 1 <= before_epoch_local[7] <= 366
assert before_epoch_local[8] == 0 or before_epoch_local[8] == 1
print("localtime()/gmtime() tests passed")

# ===========================================================
# Test strftime/strptime
# ===========================================================

sample_tm = (
    2024,
    2,
    29,
    6,
    7,
    8,
    3,
    60,
    -1,
)
formatted = time.strftime(
    "%Y-%m-%d %H:%M:%S %j %w %% %a %A %b %B", sample_tm
)
assert formatted == "2024-02-29 06:07:08 060 4 % Thu Thursday Feb February"
assert time.strftime("%Y", epoch_gm) == "1970"
assert time.strftime("X%QY", sample_tm) == "X%QY"

parsed_full = time.strptime(
    "2024-02-29 06:07:08", "%Y-%m-%d %H:%M:%S"
)
assert parsed_full[0] == 2024
assert parsed_full[1] == 2
assert parsed_full[2] == 29
assert parsed_full[3] == 6
assert parsed_full[4] == 7
assert parsed_full[5] == 8
assert parsed_full[7] == 60
assert parsed_full[8] == -1

parsed_yday = time.strptime(
    "2024-060", "%Y-%j"
)
assert parsed_yday[0] == 2024
assert parsed_yday[1] == 2
assert parsed_yday[2] == 29
assert parsed_yday[7] == 60

parsed_wday = time.strptime(
    "2024-02-29 4", "%Y-%m-%d %w"
)
assert parsed_wday[6] == 3

parsed_percent = time.strptime("2024%060", "%Y%%%j")
assert parsed_percent[0] == 2024
assert parsed_percent[1] == 2
assert parsed_percent[2] == 29
assert parsed_percent[7] == 60

parsed_defaults = time.strptime("05:06:07", "%H:%M:%S")
assert parsed_defaults[0] == 1900
assert parsed_defaults[1] == 1
assert parsed_defaults[2] == 1
assert parsed_defaults[3] == 5
assert parsed_defaults[4] == 6
assert parsed_defaults[5] == 7
assert parsed_defaults[6] == 0
assert parsed_defaults[7] == 1
assert parsed_defaults[8] == -1

epoch_fmt = time.strftime("%Y-%m-%d %H:%M:%S %w %j", epoch_gm)
assert epoch_fmt == "1970-01-01 00:00:00 4 001"
epoch_roundtrip = time.strptime(epoch_fmt, "%Y-%m-%d %H:%M:%S %w %j")
assert epoch_roundtrip[0] == 1970
assert epoch_roundtrip[1] == 1
assert epoch_roundtrip[2] == 1
assert epoch_roundtrip[3] == 0
assert epoch_roundtrip[4] == 0
assert epoch_roundtrip[5] == 0
assert epoch_roundtrip[6] == 3
assert epoch_roundtrip[7] == 1
assert epoch_roundtrip[8] == -1
print("strftime()/strptime() tests passed")

# ===========================================================
# Test from-import forms for new time APIs
# ===========================================================

from time import gmtime, localtime, strftime, strptime

import_gm = gmtime(0.0)
import_local = localtime(0.0)
assert import_gm[0] == 1970
assert import_local[0] > 0
assert 1 <= import_local[1] <= 12
assert 1 <= import_local[2] <= 31
assert import_local[8] == 0 or import_local[8] == 1
assert strftime("%Y-%m-%d", import_gm) == "1970-01-01"

import_parsed = strptime(
    "1970-01-01", "%Y-%m-%d"
)
assert import_parsed[0] == 1970
assert import_parsed[1] == 1
assert import_parsed[2] == 1
print("from-import tuple/date-format tests passed")

print("All time module tests passed!")
