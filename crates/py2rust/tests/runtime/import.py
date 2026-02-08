# Test import statements

# Test 1: from os import remove
f = open("/tmp/test_aot_from_import.txt", "w")
f.write("from import remove\n")
f.close()

from os import remove

remove("/tmp/test_aot_from_import.txt")

# Test 2: from os import remove as rm
f = open("/tmp/test_aot_from_import_alias.txt", "w")
f.write("from import remove alias\n")
f.close()

from os import remove as rm

rm("/tmp/test_aot_from_import_alias.txt")

# Test 3: import os as o
f = open("/tmp/test_aot_import_alias.txt", "w")
f.write("import os alias\n")
f.close()

import os as o

o.remove("/tmp/test_aot_import_alias.txt")

# Test 4: from math import *
from math import *

assert sqrt(16.0) == 4.0, "sqrt(16.0) should equal 4.0"
assert ceil(3.2) == 4, "ceil(3.2) should equal 4"
assert floor(3.8) == 3, "floor(3.8) should equal 3"

print("All import tests passed!")
