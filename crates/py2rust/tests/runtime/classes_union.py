# Union method calls - methods shared across all variants

class CircleU:
    r: float
    def __init__(self, r: float) -> None:
        self.r = r
    def describe(self) -> str:
        return "CircleU"
    def area(self) -> float:
        return 3.14 * self.r * self.r
    def scale(self, factor: float) -> float:
        return self.area() * factor

class RectU:
    w: float
    h: float
    def __init__(self, w: float, h: float) -> None:
        self.w = w
        self.h = h
    def describe(self) -> str:
        return "RectU"
    def area(self) -> float:
        return self.w * self.h
    def scale(self, factor: float) -> float:
        return self.area() * factor

ShapeU = CircleU | RectU

def get_description(s: ShapeU) -> str:
    return s.describe()

def get_area(s: ShapeU) -> float:
    return s.area()

# Test with CircleU variant
c: ShapeU = CircleU(2.0)
assert get_description(c) == "CircleU", "CircleU description"
assert get_area(c) == 12.56, "CircleU area"

# Test with RectU variant
r: ShapeU = RectU(3.0, 4.0)
assert get_description(r) == "RectU", "RectU description"
assert get_area(r) == 12.0, "RectU area"

# Test method call in f-string
def format_shape(s: ShapeU) -> str:
    a: float = s.area()
    return f"{s.describe()}: {a}"

assert format_shape(c) == "CircleU: 12.56", "f-string with CircleU method"
assert format_shape(r) == "RectU: 12.0", "f-string with RectU method"

# Test direct method calls (not through function)
circle_direct: ShapeU = CircleU(5.0)
rect_direct: ShapeU = RectU(2.0, 3.0)
assert circle_direct.describe() == "CircleU", "direct CircleU method"
assert rect_direct.describe() == "RectU", "direct RectU method"
assert circle_direct.area() == 78.5, "direct CircleU area"
assert rect_direct.area() == 6.0, "direct RectU area"

# Test methods with parameters
def scale_shape(s: ShapeU, f: float) -> float:
    return s.scale(f)

c_scale: ShapeU = CircleU(2.0)
r_scale: ShapeU = RectU(3.0, 4.0)
assert scale_shape(c_scale, 2.0) == 25.12, "CircleU scale via function"
assert scale_shape(r_scale, 3.0) == 36.0, "RectU scale via function"
assert c_scale.scale(0.5) == 6.28, "CircleU scale direct"
assert r_scale.scale(0.5) == 6.0, "RectU scale direct"

# Test methods returning different types
class StatusOk:
    code: int
    def __init__(self, c: int) -> None:
        self.code = c
    def is_success(self) -> bool:
        return True
    def get_code(self) -> int:
        return self.code

class StatusErr:
    code: int
    def __init__(self, c: int) -> None:
        self.code = c
    def is_success(self) -> bool:
        return False
    def get_code(self) -> int:
        return self.code

Status = StatusOk | StatusErr

ok: Status = StatusOk(200)
err: Status = StatusErr(404)

assert ok.is_success() == True, "StatusOk is_success"
assert err.is_success() == False, "StatusErr is_success"
assert ok.get_code() == 200, "StatusOk get_code"
assert err.get_code() == 404, "StatusErr get_code"

# Test method calls in expressions
total: int = ok.get_code() + err.get_code()
assert total == 604, "Union method in expression"

# Test method calls in conditionals
def check_status(s: Status) -> str:
    if s.is_success():
        return "OK"
    else:
        return "ERROR"

assert check_status(ok) == "OK", "StatusOk in conditional"
assert check_status(err) == "ERROR", "StatusErr in conditional"

print("Union method tests passed!")

# Test that docstrings are allowed and ignored in classes and methods

class Calculator:
    """A simple calculator class"""
    version: int = 100

    def __init__(self, id: int) -> None:
        """Initialize calculator with an ID"""
        self.id: int = id

    def add(self, a: int, b: int) -> int:
        """Add two numbers"""
        return a + b

    def subtract(self, a: int, b: int) -> int:
        """
        Subtract b from a

        Multi-line docstring test
        """
        return a - b

    def get_id(self) -> int:
        """Get calculator ID"""
        return self.id

    @staticmethod
    def multiply(a: int, b: int) -> int:
        """Multiply two numbers"""
        return a * b

calc = Calculator(42)
assert calc.add(2, 3) == 5, "add should work"
assert calc.subtract(10, 3) == 7, "subtract should work"
assert calc.get_id() == 42, "get_id should work"
assert Calculator.multiply(4, 5) == 20, "static method should work"
assert Calculator.version == 100, "class attribute should work"

print("Docstring tests passed!")

# Test @classmethod functionality

class Counter:
    """Test classmethod with class state"""
    count: int = 0
    multiplier: int = 1

    def __init__(self) -> None:
        Counter.count = Counter.count + 1

    @classmethod
    def get_count(cls) -> int:
        """Get current count"""
        return Counter.count

    @classmethod
    def reset(cls) -> None:
        """Reset counter to zero"""
        Counter.count = 0

    @classmethod
    def set_multiplier(cls, m: int) -> None:
        """Set multiplier value"""
        Counter.multiplier = m

    @classmethod
    def get_multiplied_count(cls) -> int:
        """Get count multiplied by multiplier"""
        return Counter.count * Counter.multiplier

# Test classmethod calls
c1 = Counter()
c2 = Counter()
assert Counter.get_count() == 2, "get_count should return 2"

Counter.reset()
assert Counter.get_count() == 0, "get_count should return 0 after reset"

c3 = Counter()
c4 = Counter()
assert Counter.get_count() == 2, "get_count should return 2 again"

# Test classmethod with parameters
Counter.set_multiplier(5)
assert Counter.get_multiplied_count() == 10, "multiplied count should be 10"

Counter.set_multiplier(3)
assert Counter.get_multiplied_count() == 6, "multiplied count should be 6"

# Test multiple classmethods with different return types
class Config:
    """Test classmethods with different return types"""
    enabled: bool = True
    max_items: int = 100
    name: str = "default"

    @classmethod
    def is_enabled(cls) -> bool:
        """Check if enabled"""
        return Config.enabled

    @classmethod
    def get_max(cls) -> int:
        """Get max items"""
        return Config.max_items

    @classmethod
    def get_name(cls) -> str:
        """Get config name"""
        return Config.name

    @classmethod
    def toggle(cls) -> None:
        """Toggle enabled state"""
        Config.enabled = not Config.enabled

assert Config.is_enabled() == True, "should be enabled"
assert Config.get_max() == 100, "max should be 100"
assert Config.get_name() == "default", "name should be default"

Config.toggle()
assert Config.is_enabled() == False, "should be disabled after toggle"

Config.toggle()
assert Config.is_enabled() == True, "should be enabled after second toggle"

print("Classmethod tests passed!")

# Test pattern matching with __match_args__

class Point2D:
    """2D point with pattern matching support"""
    __match_args__ = ('x', 'y')
    x: int
    y: int

    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

class Point3D:
    """3D point with pattern matching support"""
    __match_args__ = ('x', 'y', 'z')
    x: int
    y: int
    z: int

    def __init__(self, x: int, y: int, z: int) -> None:
        self.x = x
        self.y = y
        self.z = z

Coordinate = Point2D | Point3D

def describe_point(p: Coordinate) -> str:
    """Describe a point using pattern matching"""
    match p:
        case Point2D(x, y):
            return f"2D point at ({x}, {y})"
        case Point3D(x, y, z):
            return f"3D point at ({x}, {y}, {z})"

p2: Coordinate = Point2D(3, 4)
p3: Coordinate = Point3D(1, 2, 3)

assert describe_point(p2) == "2D point at (3, 4)", "2D point description"
assert describe_point(p3) == "3D point at (1, 2, 3)", "3D point description"

# Test with different values
p2_origin: Coordinate = Point2D(0, 0)
assert describe_point(p2_origin) == "2D point at (0, 0)", "origin 2D"

p3_negative: Coordinate = Point3D(-1, -2, -3)
assert describe_point(p3_negative) == "3D point at (-1, -2, -3)", "negative 3D"

print("Pattern matching tests passed!")
