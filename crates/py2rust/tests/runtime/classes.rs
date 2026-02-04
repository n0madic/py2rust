//! Runtime tests for classes and objects.

use crate::common::run_py;

#[test]
fn runtime_classes_comprehensive() {
    run_py(
        "classes",
        r#"
# Basic class with fields and methods
class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x: int = x
        self.y: int = y

    def sum(self) -> int:
        return self.x + self.y

    def distance_squared(self) -> int:
        return self.x * self.x + self.y * self.y

# Class with computed values
class Rectangle:
    def __init__(self, width: int, height: int) -> None:
        self.width: int = width
        self.height: int = height

    def area(self) -> int:
        return self.width * self.height

    def perimeter(self) -> int:
        return 2 * (self.width + self.height)

# Test Point
p: Point = Point(3, 4)
assert p.x == 3
assert p.y == 4
assert p.sum() == 7
assert p.distance_squared() == 25

p2: Point = Point(10, 20)
assert p2.x == 10
assert p2.sum() == 30

# Test Rectangle
r: Rectangle = Rectangle(5, 10)
assert r.width == 5
assert r.height == 10
assert r.area() == 50
assert r.perimeter() == 30

print("All class tests passed!")
"#,
        Some("All class tests passed!"),
    );
}
