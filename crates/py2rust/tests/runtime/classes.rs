//! Runtime tests for classes and objects.

use crate::common::run_py;

#[test]
fn runtime_classes_comprehensive() {
    run_py(
        "classes",
        r#"
# Consolidated test file for classes and OOP

from typing import Any

# ===== SECTION: Class definitions and __init__ =====

class Point:
    x: int
    y: int

    def __init__(self, x: int, y: int):
        self.x = x
        self.y = y

    def distance_squared(self) -> int:
        return self.x * self.x + self.y * self.y

    def add(self, other_x: int, other_y: int) -> int:
        return self.x + other_x + self.y + other_y

# Create instance via constructor
p = Point(3, 4)

# ===== SECTION: Instance fields and methods =====

# Test field access
assert p.x == 3, "Field x should be 3"
assert p.y == 4, "Field y should be 4"

# Test method calls
d = p.distance_squared()
assert d == 25, "distance_squared should be 25 (3*3 + 4*4)"

s = p.add(2, 3)
assert s == 12, "add should be 12 (3+2 + 4+3)"

# Test field modification
p.x = 10
assert p.x == 10, "Field x should be 10 after modification"

# ===== SECTION: Optional attribute access =====

class TypedPoint:
    x: int
    y: int

    def __init__(self, x: int, y: int):
        self.x = x
        self.y = y

def maybe_typed_point(flag: bool) -> TypedPoint | None:
    if flag:
        return TypedPoint(1, 2)
    return None

opt_p: TypedPoint | None = maybe_typed_point(True)
if opt_p is not None:
    # Accessing a field after an is-not-None guard should be safe.
    assert opt_p.x == 1, "opt_p.x should equal 1 after is_none guard"

# ===== SECTION: Multiple instances =====

# Create another instance
p2 = Point(5, 12)
assert p2.x == 5, "p2.x should be 5"
assert p2.y == 12, "p2.y should be 12"
assert p2.distance_squared() == 169, "p2.distance_squared should be 169"

# Original instance unchanged (except for modification)
assert p.x == 10, "p.x should still be 10"
assert p.y == 4, "p.y should still be 4"

# ===== SECTION: Class attributes =====

class AttrCounter:
    count = 0
    name = "Counter"

# Test basic access
assert AttrCounter.count == 0, "AttrCounter.count should equal 0"
assert AttrCounter.name == "Counter", "AttrCounter.name should equal \"Counter\""

# Test modification
AttrCounter.count = 5
assert AttrCounter.count == 5, "AttrCounter.count should equal 5"

class Tracker:
    total = 0

    def __init__(self):
        Tracker.total += 1

t1 = Tracker()
assert Tracker.total == 1, "Tracker.total should equal 1"
t2 = Tracker()
assert Tracker.total == 2, "Tracker.total should equal 2"

# Test float class attribute
class Config:
    rate = 0.5

assert Config.rate == 0.5, "Config.rate should equal 0.5"
Config.rate = 1.5
assert Config.rate == 1.5, "Config.rate should equal 1.5"

# Test bool class attribute
class Flags:
    enabled = True
    debug = False

assert Flags.enabled == True, "Flags.enabled should equal True"
assert Flags.debug == False, "Flags.debug should equal False"
Flags.debug = True
assert Flags.debug == True, "Flags.debug should equal True"

# Test class attr with multiple classes
class AttrA:
    x = 10

class AttrB:
    x = 20

assert AttrA.x == 10, "AttrA.x should equal 10"
assert AttrB.x == 20, "AttrB.x should equal 20"
AttrA.x = 15
assert AttrA.x == 15, "AttrA.x should equal 15"
assert AttrB.x == 20, "AttrB.x should equal 20"  # B.x should be unchanged

# ===== SECTION: Single inheritance =====

class Animal:
    name: str
    def __init__(self, name: str):
        self.name = name
    def speak(self) -> str:
        return "..."

class Dog(Animal):
    def __init__(self, name: str):
        super().__init__(name)
    def speak(self) -> str:
        return "Woof!"

class Cat(Animal):
    def __init__(self, name: str):
        super().__init__(name)
    def speak(self) -> str:
        return "Meow!"

# Test basic inheritance
dog = Dog("Rex")
assert dog.name == "Rex", "dog.name should equal \"Rex\""
assert dog.speak() == "Woof!", "dog.speak() should equal \"Woof!\""

cat = Cat("Whiskers")
assert cat.name == "Whiskers", "cat.name should equal \"Whiskers\""
assert cat.speak() == "Meow!", "cat.speak() should equal \"Meow!\""

# ===== SECTION: super().__init__() =====

class Shape:
    x: int
    y: int
    def __init__(self, x: int, y: int):
        self.x = x
        self.y = y
    def describe(self) -> str:
        return "Shape"

class Circle(Shape):
    radius: int
    def __init__(self, x: int, y: int, r: int):
        super().__init__(x, y)
        self.radius = r
    def describe(self) -> str:
        return "Circle"

circle = Circle(10, 20, 5)
assert circle.x == 10, "circle.x should equal 10"
assert circle.y == 20, "circle.y should equal 20"
assert circle.radius == 5, "circle.radius should equal 5"
assert circle.describe() == "Circle", "circle.describe() should equal \"Circle\""

# ===== SECTION: isinstance() for primitives =====

inst_x: int = 42
assert isinstance(inst_x, int), "isinstance(inst_x, int) should be True"
assert not isinstance(inst_x, str), "assertion failed: not isinstance(inst_x, str)"
assert not isinstance(inst_x, float), "assertion failed: not isinstance(inst_x, float)"
assert not isinstance(inst_x, bool), "assertion failed: not isinstance(inst_x, bool)"

inst_y: float = 3.14
assert isinstance(inst_y, float), "isinstance(inst_y, float) should be True"
assert not isinstance(inst_y, int), "assertion failed: not isinstance(inst_y, int)"
assert not isinstance(inst_y, str), "assertion failed: not isinstance(inst_y, str)"

flag: bool = True
assert isinstance(flag, bool), "isinstance(flag, bool) should be True"
assert isinstance(flag, int), "isinstance(flag, int) should be True (bool is subclass of int)"
assert not isinstance(flag, str), "assertion failed: not isinstance(flag, str)"

# ===== SECTION: isinstance() for user classes =====

class IsPoint:
    x: int
    y: int
    def __init__(self, x: int, y: int):
        self.x = x
        self.y = y

class IsCircle:
    r: int
    def __init__(self, r: int):
        self.r = r

is_p = IsPoint(1, 2)
is_c = IsCircle(5)

assert isinstance(is_p, IsPoint), "isinstance(is_p, IsPoint) should be True"
assert not isinstance(is_p, IsCircle), "assertion failed: not isinstance(is_p, IsCircle)"
assert isinstance(is_c, IsCircle), "isinstance(is_c, IsCircle) should be True"
assert not isinstance(is_c, IsPoint), "assertion failed: not isinstance(is_c, IsPoint)"

# Check class vs primitive type
assert not isinstance(is_p, int), "assertion failed: not isinstance(is_p, int)"
assert not isinstance(is_p, str), "assertion failed: not isinstance(is_p, str)"
assert not isinstance(inst_x, IsPoint), "assertion failed: not isinstance(inst_x, IsPoint)"

# ===== SECTION: isinstance() with inheritance =====

assert isinstance(dog, Dog), "isinstance(dog, Dog) should be True"
assert isinstance(dog, Animal), "isinstance(dog, Animal) should be True"
assert isinstance(cat, Cat), "isinstance(cat, Cat) should be True"
assert isinstance(cat, Animal), "isinstance(cat, Animal) should be True"

# Not a cat
assert not isinstance(dog, Cat), "assertion failed: not isinstance(dog, Cat)"
# Not a dog
assert not isinstance(cat, Dog), "assertion failed: not isinstance(cat, Dog)"

# Shape inheritance
assert isinstance(circle, Circle), "isinstance(circle, Circle) should be True"
assert isinstance(circle, Shape), "isinstance(circle, Shape) should be True"

# ===== SECTION: Virtual method dispatch (polymorphism) =====

class DispatchAnimal:
    def speak(self) -> str:
        return "..."

class DispatchDog(DispatchAnimal):
    def speak(self) -> str:
        return "Woof!"

class DispatchCat(DispatchAnimal):
    def speak(self) -> str:
        return "Meow!"

# Test direct method calls work via vtable dispatch
dispatch_dog = DispatchDog()
dispatch_cat = DispatchCat()
dispatch_animal = DispatchAnimal()

assert dispatch_dog.speak() == "Woof!", "dispatch_dog.speak() should equal \"Woof!\""
assert dispatch_cat.speak() == "Meow!", "dispatch_cat.speak() should equal \"Meow!\""
assert dispatch_animal.speak() == "...", "dispatch_animal.speak() should equal \"...\""

# Test multi-level inheritance
class Puppy(DispatchDog):
    def speak(self) -> str:
        return "Yip!"

puppy = Puppy()
assert puppy.speak() == "Yip!", "puppy.speak() should equal \"Yip!\""

# Test three-level inheritance
class Chihuahua(Puppy):
    def speak(self) -> str:
        return "Bark!"

chihuahua = Chihuahua()
assert chihuahua.speak() == "Bark!", "chihuahua.speak() should equal \"Bark!\""

# ===== SECTION: User-defined decorators =====

def identity(func) -> Any:
    return func

@identity
def simple(a: int, b: int) -> int:
    return a + b

result_deco = simple(3, 4)
assert result_deco == 7, "identity decorator failed"

# Multiple identity decorators
def identity2(func) -> Any:
    return func

@identity
@identity2
def add_deco(x: int, y: int) -> int:
    return x + y

result_deco2 = add_deco(10, 20)
assert result_deco2 == 30, "multiple identity decorators failed"

# Decorator on function with default args
@identity
def greet(name: str, greeting: str = "Hello") -> str:
    return greeting + " " + name

result_deco3 = greet("World")
assert result_deco3 == "Hello World", "decorator with defaults failed"

result_deco3b = greet("World", "Hi")
assert result_deco3b == "Hi World", "decorator with explicit arg failed"

# ===== SECTION: Wrapper decorators =====
# Wrapper decorators return a closure that wraps the original function

def double_result(func):
    def wrapper(x: int) -> int:
        return func(x) * 2
    return wrapper

@double_result
def get_value(n: int) -> int:
    return n + 5

wrapper_result1 = get_value(10)
assert wrapper_result1 == 30, "wrapper decorator (10+5)*2 should be 30"

wrapper_result2 = get_value(0)
assert wrapper_result2 == 10, "wrapper decorator (0+5)*2 should be 10"

# String wrapper decorator
def add_prefix(func):
    def wrapper(name: str) -> str:
        return "Hello, " + func(name)
    return wrapper

@add_prefix
def greet_person(name: str) -> str:
    return name + "!"

wrapper_str1 = greet_person("World")
assert wrapper_str1 == "Hello, World!", "wrapper string decorator failed"

wrapper_str2 = greet_person("Alice")
assert wrapper_str2 == "Hello, Alice!", "wrapper string decorator with Alice failed"

# ===== SECTION: @property decorator (getter/setter) =====

class PropCounter:
    _value: int

    def __init__(self, v: int):
        self._value = v

    @property
    def value(self) -> int:
        return self._value

    @value.setter
    def value(self, v: int) -> None:
        self._value = v

    @property
    def doubled(self) -> int:
        return self._value * 2

# Test property getter
prop_c = PropCounter(5)
assert prop_c.value == 5, "prop_c.value should equal 5"
assert prop_c.doubled == 10, "prop_c.doubled should equal 10"

# Test property setter
prop_c.value = 10
assert prop_c.value == 10, "prop_c.value should equal 10"
assert prop_c.doubled == 20, "prop_c.doubled should equal 20"

# Test read-only property (no setter)
class Rectangle:
    _width: int
    _height: int

    def __init__(self, w: int, h: int):
        self._width = w
        self._height = h

    @property
    def area(self) -> int:
        return self._width * self._height

rect = Rectangle(3, 4)
assert rect.area == 12, "rect.area should equal 12"

# ===== SECTION: @staticmethod decorator =====

class StaticMath:
    @staticmethod
    def static_add(a: int, b: int) -> int:
        return a + b

    @staticmethod
    def static_multiply(x: int, y: int) -> int:
        return x * y

# Test calling static method on class
assert StaticMath.static_add(2, 3) == 5, "StaticMath.static_add(2, 3) should equal 5"
assert StaticMath.static_multiply(4, 5) == 20, "StaticMath.static_multiply(4, 5) should equal 20"

# Test calling static method on instance
sm = StaticMath()
assert sm.static_add(10, 20) == 30, "sm.static_add(10, 20) should equal 30"
assert sm.static_multiply(6, 7) == 42, "sm.static_multiply(6, 7) should equal 42"

# Static method with no arguments
class StaticCounter:
    @staticmethod
    def get_default() -> int:
        return 100

assert StaticCounter.get_default() == 100, "StaticCounter.get_default() should equal 100"
sc = StaticCounter()
assert sc.get_default() == 100, "sc.get_default() should equal 100"

# ===== SECTION: @classmethod decorator =====

# Basic classmethod - cls is passed as first argument (as class_id integer)
class ClassMethodBasic:
    count: int = 0  # Class attribute with type annotation

    @classmethod
    def increment(cls: int) -> int:
        # cls receives the class_id as an integer
        ClassMethodBasic.count = ClassMethodBasic.count + 1
        return ClassMethodBasic.count

    @classmethod
    def get_count(cls: int) -> int:
        return ClassMethodBasic.count

# Test calling classmethod on class
assert ClassMethodBasic.get_count() == 0, "ClassMethodBasic.get_count() should equal 0"
result = ClassMethodBasic.increment()
assert result == 1, "result should equal 1"
assert ClassMethodBasic.get_count() == 1, "ClassMethodBasic.get_count() should equal 1"
ClassMethodBasic.increment()
assert ClassMethodBasic.get_count() == 2, "ClassMethodBasic.get_count() should equal 2"

# Test calling classmethod on instance
obj = ClassMethodBasic()
result2 = obj.increment()
assert result2 == 3, "result2 should equal 3"
assert obj.get_count() == 3, "obj.get_count() should equal 3"

# Classmethod with additional parameters
class ClassMethodWithArgs:
    value: int = 10  # Class attribute with type annotation

    @classmethod
    def add_to_value(cls: int, x: int) -> int:
        return ClassMethodWithArgs.value + x

    @classmethod
    def multiply_value(cls: int, x: int, y: int) -> int:
        return ClassMethodWithArgs.value * x * y

# Test classmethod with args on class
assert ClassMethodWithArgs.add_to_value(5) == 15, "ClassMethodWithArgs.add_to_value(5) should equal 15"
assert ClassMethodWithArgs.multiply_value(2, 3) == 60, "ClassMethodWithArgs.multiply_value(2, 3) should equal 60"

# Test classmethod with args on instance
cwa = ClassMethodWithArgs()
assert cwa.add_to_value(20) == 30, "cwa.add_to_value(20) should equal 30"
assert cwa.multiply_value(4, 5) == 200, "cwa.multiply_value(4, 5) should equal 200"

# Classmethod returning different types
class ClassMethodTypes:
    name = "TestClass"  # Class attribute with type annotation

    @classmethod
    def get_name(cls: int) -> str:
        return ClassMethodTypes.name

    @classmethod
    def is_valid(cls: int) -> bool:
        return True

assert ClassMethodTypes.get_name() == "TestClass", "ClassMethodTypes.get_name() should equal \"TestClass\""
assert ClassMethodTypes.is_valid() == True, "ClassMethodTypes.is_valid() should equal True"

# Mixed static and class methods in same class
class MixedMethods:
    counter: int = 0  # Class attribute with type annotation

    @staticmethod
    def static_helper(x: int) -> int:
        return x * 2

    @classmethod
    def class_increment(cls: int) -> int:
        MixedMethods.counter = MixedMethods.counter + 1
        return MixedMethods.counter

    def instance_method(self) -> int:
        return MixedMethods.counter + 100

# Test all three method types
assert MixedMethods.static_helper(5) == 10, "MixedMethods.static_helper(5) should equal 10"
assert MixedMethods.class_increment() == 1, "MixedMethods.class_increment() should equal 1"
mm = MixedMethods()
assert mm.instance_method() == 101, "mm.instance_method() should equal 101"
assert mm.static_helper(7) == 14, "mm.static_helper(7) should equal 14"
assert mm.class_increment() == 2, "mm.class_increment() should equal 2"

# Test annotated assignment with value is treated as class attribute (not instance field)
class AnnotatedClassAttr:
    count: int = 0
    name: str = "test"
    flag: bool = True

    @classmethod
    def increment_count(cls: int) -> int:
        AnnotatedClassAttr.count = AnnotatedClassAttr.count + 1
        return AnnotatedClassAttr.count

# Verify class attributes are accessible and mutable
assert AnnotatedClassAttr.count == 0, "AnnotatedClassAttr.count should equal 0"
assert AnnotatedClassAttr.name == "test", "AnnotatedClassAttr.name should equal \"test\""
assert AnnotatedClassAttr.flag == True, "AnnotatedClassAttr.flag should equal True"
assert AnnotatedClassAttr.increment_count() == 1, "AnnotatedClassAttr.increment_count() should equal 1"
assert AnnotatedClassAttr.count == 1, "AnnotatedClassAttr.count should equal 1"
AnnotatedClassAttr.count = 100
assert AnnotatedClassAttr.count == 100, "AnnotatedClassAttr.count should equal 100"

print("@classmethod tests passed!")
"#,
        Some("@classmethod tests passed!"),
    );
}
