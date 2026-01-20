# Q 语言教程 - Trait

## 目录

1. [Trait 概述](#trait-概述)
2. [定义 Trait](#定义-trait)
3. [使用 Trait](#使用-trait)
4. [默认实现](#默认实现)
5. [Trait 与接口的区别](#trait-与接口的区别)
6. [实现状态说明](#实现状态说明)

---

## Trait 概述

Trait 是 Q 语言借鉴 Rust 的特性，提供了比接口更强大的代码复用机制。

### Trait vs Interface

| 特性 | Interface | Trait |
|------|-----------|-------|
| 方法签名 | ✅ | ✅ |
| 默认实现 | ❌ | ✅ |
| 多重实现 | ✅ | ✅ |
| 代码复用 | 低 | 高 |

### 为什么需要 Trait

```q
// 使用接口：每个类都需要实现所有方法
interface Printable {
    func print()
    func println()
    func debug()
}

class User implements Printable {
    func print() { /* 实现 */ }
    func println() { /* 实现 */ }
    func debug() { /* 实现 */ }  // 重复代码
}

class Product implements Printable {
    func print() { /* 实现 */ }
    func println() { /* 实现 */ }
    func debug() { /* 实现 */ }  // 重复代码
}

// 使用 Trait：提供默认实现
trait Printable {
    func format() string  // 抽象方法
    
    func print() {  // 默认实现
        print(this.format())
    }
    
    func println() {  // 默认实现
        println(this.format())
    }
}

class User {
    use Printable
    
    func format() string {  // 只需实现这一个方法
        return "User(...)"
    }
}
```

---

## 定义 Trait

### 基本语法

```q
trait TraitName {
    // 抽象方法（必须实现）
    func abstractMethod()
    
    // 默认实现（可选重写）
    func defaultMethod() {
        // 默认行为
    }
}
```

### 简单 Trait

```q
trait Printable {
    // 抽象方法：子类必须实现
    func format() string
    
    // 默认实现：子类可以直接使用
    func print() {
        print(this.format())
    }
    
    func println() {
        println(this.format())
    }
}
```

### 带泛型的 Trait

```q
trait Comparable<T> {
    // 抽象方法
    func compareTo(other: T) int
    
    // 默认实现
    func lessThan(other: T) bool {
        return this.compareTo(other) < 0
    }
    
    func greaterThan(other: T) bool {
        return this.compareTo(other) > 0
    }
    
    func equals(other: T) bool {
        return this.compareTo(other) == 0
    }
}
```

---

## 使用 Trait

### use 关键字

使用 `use` 关键字将 Trait 混入到类中：

```q
trait Printable {
    func format() string
    
    func print() {
        println(this.format())
    }
}

class User {
    func init(var name: string, var age: int) {}
    
    // 使用 Trait
    use Printable
    
    // 实现抽象方法
    func format() string {
        return "User(" + this.name + ", " + this.age as string + ")"
    }
}

// 使用
var user = new User("Alice", 25)
user.print()  // 调用 Trait 的默认方法
```

### 实现示例

```q
println("=== Trait Implementation Test ===")

trait Printable {
    func print()
}

class Document {
    // 使用 Trait
    use Printable
    
    content: string = "Hello"
    
    // 实现 Trait 方法
    func print() {
        println("Document content")
    }
}

var doc = new Document()
doc.print()  // 输出：Document content

println("=== Test Passed! ===")
```

---

## 默认实现

### 提供通用行为

```q
trait Comparable<T> {
    // 核心方法（必须实现）
    func compareTo(other: T) int
    
    // 衍生方法（自动获得）
    func lessThan(other: T) bool {
        return this.compareTo(other) < 0
    }
    
    func greaterThan(other: T) bool {
        return this.compareTo(other) > 0
    }
    
    func lessOrEqual(other: T) bool {
        return this.compareTo(other) <= 0
    }
    
    func greaterOrEqual(other: T) bool {
        return this.compareTo(other) >= 0
    }
    
    func equals(other: T) bool {
        return this.compareTo(other) == 0
    }
}

class Person {
    func init(var name: string, var age: int) {}
    
    use Comparable<Person>
    
    // 只需实现 compareTo
    func compareTo(other: Person) int {
        return this.age - other.age
    }
    
    // 自动获得其他 5 个方法！
}

var p1 = new Person("Alice", 25)
var p2 = new Person("Bob", 30)

println(p1.lessThan(p2))        // true
println(p1.greaterThan(p2))     // false
println(p1.equals(p2))          // false
```

### 重写默认实现

可以重写 Trait 的默认实现：

```q
trait Printable {
    func format() string
    
    func print() {
        println(this.format())
    }
    
    func debug() {
        println("[DEBUG] " + this.format())
    }
}

class User {
    func init(var name: string) {}
    
    use Printable
    
    func format() string {
        return "User: " + this.name
    }
    
    // 重写默认实现
    func debug() {
        println(">>> DEBUG: " + this.name + " <<<")
    }
}
```

---

## 多个 Trait

### 使用多个 Trait

一个类可以使用多个 Trait：

```q
trait Printable {
    func format() string
    
    func print() {
        println(this.format())
    }
}

trait Comparable<T> {
    func compareTo(other: T) int
    
    func lessThan(other: T) bool {
        return this.compareTo(other) < 0
    }
}

class User {
    func init(var name: string, var age: int) {}
    
    // 使用多个 Trait
    use Printable
    use Comparable<User>
    
    func format() string {
        return "User(" + this.name + ", " + this.age as string + ")"
    }
    
    func compareTo(other: User) int {
        return this.age - other.age
    }
}

var u1 = new User("Alice", 25)
var u2 = new User("Bob", 30)

u1.print()              // 来自 Printable
println(u1.lessThan(u2))  // 来自 Comparable
```

---

## Trait 与接口的区别

### Interface（接口）

```q
// 接口：只有方法签名
interface Drawable {
    func draw()
    func getBounds() Rectangle
}

// 实现：必须实现所有方法
class Circle implements Drawable {
    func draw() {
        // 实现
    }
    
    func getBounds() Rectangle {
        // 实现
    }
}
```

### Trait（特质）

```q
// Trait：有默认实现
trait Drawable {
    func getPosition() Point
    
    func draw() {
        var pos = this.getPosition()
        println("Drawing at " + pos.x as string + ", " + pos.y as string)
    }
}

// 使用：只需实现抽象方法
class Circle {
    func init(var x: int, var y: int) {}
    
    use Drawable
    
    func getPosition() Point {
        return Point { x: this.x, y: this.y }
    }
    
    // draw() 自动获得！
}
```

### 何时使用

| 使用场景 | 选择 |
|---------|------|
| 纯契约定义 | Interface |
| 需要默认行为 | Trait |
| 需要多态 | Interface 或 Trait |
| 需要代码复用 | Trait |

---

## 实现状态说明

**重要提示**：根据测试用例 `trait_check_test.q`，Trait 功能当前处于以下状态：

### ✅ 已支持（基础语法）

1. **Trait 定义**：`trait TraitName {}`
2. **use 关键字**：`use TraitName`
3. **Trait 方法定义**
4. **类中实现 Trait 方法**

### 🚧 可能未完全实现

1. **默认实现**：Trait 中的默认方法实现
2. **泛型 Trait**：`trait Comparable<T>`
3. **多个 Trait**：同时使用多个 Trait
4. **Trait 约束**：泛型类型约束中使用 Trait

### 当前可用功能

```q
// ✅ 可以定义和使用 Trait
trait Printable {
    func print()
}

class Document {
    use Printable
    
    func print() {
        println("Document content")
    }
}

var doc = new Document()
doc.print()
```

---

## 完整示例

### 示例 1：日志 Trait

```q
trait Loggable {
    func getLogMessage() string
    
    func log() {
        println("[LOG] " + this.getLogMessage())
    }
    
    func warn() {
        println("[WARN] " + this.getLogMessage())
    }
    
    func error() {
        println("[ERROR] " + this.getLogMessage())
    }
}

class User {
    func init(var name: string, var email: string) {}
    
    use Loggable
    
    func getLogMessage() string {
        return "User: " + this.name + " (" + this.email + ")"
    }
}

var user = new User("Alice", "alice@example.com")
user.log()    // [LOG] User: Alice (alice@example.com)
user.warn()   // [WARN] User: Alice (alice@example.com)
user.error()  // [ERROR] User: Alice (alice@example.com)
```

### 示例 2：序列化 Trait

```q
trait Serializable {
    func toJson() string
    
    func save(filename: string) {
        var json = this.toJson()
        // 保存到文件
        println("Saving to " + filename + ": " + json)
    }
}

class User {
    func init(var name: string, var age: int) {}
    
    use Serializable
    
    func toJson() string {
        return '{"name":"' + this.name + '","age":' + this.age as string + '}'
    }
}

var user = new User("Alice", 25)
user.save("user.json")
// 输出：Saving to user.json: {"name":"Alice","age":25}
```

### 示例 3：验证 Trait

```q
trait Validatable {
    func validate() bool
    
    func isValid() bool {
        return this.validate()
    }
    
    func validateOrThrow() {
        if !this.validate() {
            throw "ValidationException: Validation failed"
        }
    }
}

class Email {
    func init(var address: string) {}
    
    use Validatable
    
    func validate() bool {
        // 简化的验证逻辑
        var hasAt = false
        // 检查 @ 符号
        return true  // 简化
    }
}

var email = new Email("test@example.com")

if email.isValid() {
    println("Email is valid")
}

try {
    email.validateOrThrow()
    println("Validation passed")
} catch(e) {
    println("Validation failed: " + e)
}
```

---

## Trait 最佳实践

### 1. 单一职责

每个 Trait 应该专注于一个方面：

```q
// 好：专注于一个方面
trait Printable {
    func format() string
    func print() { /* ... */ }
}

trait Comparable<T> {
    func compareTo(other: T) int
    func lessThan(other: T) bool { /* ... */ }
}

// 不好：混合多个职责
trait Utils {
    func print()
    func compareTo(other: T) int
    func save(filename: string)
}
```

### 2. 最小化抽象方法

只要求实现核心方法，其他通过默认实现提供：

```q
// 好：只需实现一个核心方法
trait Comparable<T> {
    func compareTo(other: T) int  // 核心方法
    
    // 其他方法基于核心方法实现
    func lessThan(other: T) bool {
        return this.compareTo(other) < 0
    }
}
```

### 3. 有意义的默认实现

默认实现应该是通用且正确的：

```q
// 好：通用且正确的默认实现
trait Container {
    func size() int
    
    func isEmpty() bool {
        return this.size() == 0  // 合理的默认实现
    }
}
```

### 4. Trait 组合

通过组合小的 Trait 创建更强大的功能：

```q
trait Readable {
    func read() string
}

trait Writable {
    func write(content: string)
}

// 组合使用
class File {
    use Readable
    use Writable
    
    func read() string { /* ... */ }
    func write(content: string) { /* ... */ }
}
```

---

## 下一步

- 学习 [泛型](./泛型.md)
- 学习 [面向对象](./面向对象.md)
- 学习 [接口](./面向对象.md#接口)
