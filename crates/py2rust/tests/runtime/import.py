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

print("All import tests passed!")
