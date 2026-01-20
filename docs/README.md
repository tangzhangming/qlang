# Q 语言教程文档

欢迎来到 Q 语言教程！本教程将帮助你全面掌握 Q 语言的核心特性。

## 📚 教程目录

### 基础篇

#### 1. [变量和常量](./变量和常量.md)
掌握变量声明和作用域，包括：
- var 和 const 声明
- 类型推导
- 作用域规则和变量遮蔽
- 变量初始化和生命周期
- 全局变量 vs 局部变量

#### 2. [基本类型](./基本类型.md)
学习 Q 语言的类型系统，包括：
- 整数类型（int、i8、i16、i32、i64、uint、u8、u16、u32、u64）
- 浮点类型（f32、f64）
- 布尔类型（bool）
- 字符和字符串（char、string）
- 数组和切片
- Map 类型
- 特殊类型（void、null、unknown、dynamic）

#### 3. [运算符](./运算符.md)
全面了解 Q 语言运算符，包括：
- 算术运算符（+、-、*、/、%）
- 比较运算符（==、!=、<、>、<=、>=）
- 逻辑运算符（&&、||、!）
- 位运算符（&、|、^、~、<<、>>）
- 赋值运算符（=、+=、-=、*=、/=等）
- 自增自减运算符（++、--）
- 运算符优先级

#### 4. [控制结构](./控制结构.md)
掌握程序流程控制，包括：
- if 语句（if-else、if-else if-else）
- for 循环（C风格、条件循环、range循环、for-in）
- break 和 continue（支持标签）
- match 表达式（模式匹配）
- 函数和闭包（参数、返回值、高阶函数）
- 逻辑短路求值

### 数据结构篇

#### 5. [结构体](./结构体.md)
理解值类型的数据结构，包括：
- 结构体定义和字段
- 结构体方法
- 结构体实例化（字段初始化器）
- 结构体实现接口
- 值类型特性（栈分配、复制语义）
- 结构体与类的区别

#### 6. [面向对象](./面向对象.md)
深入学习面向对象编程，包括：
- 类的定义和使用
- 构造函数（init方法、参数属性提升）
- 字段和属性
- 实例方法和静态方法
- 继承（extends、override）
- 抽象类（abstract）
- 接口（interface、implements）
- 可见性（public、private、protected、internal）
- this 和 super 关键字

### 进阶篇

#### 7. [类型系统进阶](./类型系统进阶.md)
深入理解类型系统，包括：
- 类型转换（as运算符）
- 类型检查（is运算符）
- 可空类型（nullable types）
- 类型别名（type）
- 多态与类型
- typeof 和 sizeof

#### 8. [函数进阶](./函数进阶.md)
掌握高级函数特性，包括：
- 默认参数和可变参数
- 多返回值
- 闭包和捕获
- 高阶函数（函数作为参数和返回值）
- 递归函数（尾递归优化）
- 内置函数（print、println、typeof、sizeof、panic）

#### 9. [泛型](./泛型.md)
学习泛型编程，包括：
- 泛型类和泛型结构体
- 泛型函数
- 泛型接口
- 类型约束（bounds）
- where 子句
- 泛型实例化

#### 10. [Trait](./Trait.md)
掌握 Trait 系统，包括：
- Trait 定义和使用（use关键字）
- 默认实现
- 多个 Trait 组合
- Trait 与接口的区别
- Trait 最佳实践

### 错误处理与并发篇

#### 11. [错误处理](./错误处理.md)
学习异常处理机制，包括：
- try-catch-finally 语句
- throw 语句
- 异常类型
- 嵌套异常处理
- panic 函数
- 错误处理模式

#### 12. [并发编程](./并发编程.md)
掌握并发编程，包括：
- 协程（Goroutine）基础
- go 关键字启动协程
- Channel 通信
- 并发模式（生产者-消费者、Worker Pool、Pipeline）
- 同步原语（WaitGroup、Mutex）
- M:N 调度器

## 🚀 快速开始

### 初学者路线

如果你是初学者，建议按以下顺序学习：

1. **变量和类型**（第1-3章）
   - [变量和常量](./变量和常量.md) - 理解变量声明和作用域
   - [基本类型](./基本类型.md) - 掌握类型系统
   - [运算符](./运算符.md) - 了解各种运算符

2. **程序控制**（第4章）
   - [控制结构](./控制结构.md) - 掌握if、for、match等控制流

3. **数据结构**（第5-6章）
   - [结构体](./结构体.md) - 理解值类型
   - [面向对象](./面向对象.md) - 掌握类、继承、接口

4. **进阶特性**（第7-10章）
   - [类型系统进阶](./类型系统进阶.md) - 类型转换和检查
   - [函数进阶](./函数进阶.md) - 闭包和高阶函数
   - [泛型](./泛型.md) - 泛型编程
   - [Trait](./Trait.md) - Trait 系统

5. **实战技能**（第11-12章）
   - [错误处理](./错误处理.md) - 异常处理
   - [并发编程](./并发编程.md) - 协程和 Channel

### 有经验开发者路线

如果你有其他编程语言经验：

1. 快速浏览：[变量和常量](./变量和常量.md)、[基本类型](./基本类型.md)
2. 重点学习：[面向对象](./面向对象.md)、[Trait](./Trait.md)、[并发编程](./并发编程.md)
3. 参考查阅：其他章节作为参考手册使用

## 💡 关键特性

### 类型系统
- **静态类型**：编译时确定所有类型
- **类型推导**：支持自动推断类型
- **值类型 vs 引用类型**：结构体是值类型，类是引用类型
- **泛型支持**：类、结构体、函数都支持泛型
- **可空类型**：显式表示可空值（`T?`）

### 面向对象
- **单继承**：一个类只能继承一个父类
- **多接口**：可以实现多个接口
- **抽象类**：支持抽象方法和具体方法
- **参数属性提升**：构造函数参数自动成为字段
- **Trait 系统**：类似 Rust 的 Trait，支持默认实现

### 控制流
- **match 表达式**：强大的模式匹配
- **for 循环多样化**：支持多种循环形式
- **短路求值**：逻辑运算符支持短路
- **异常处理**：try-catch-finally 机制

### 并发编程
- **轻量级协程**：类似 Go 的 goroutine
- **M:N 调度器**：高效的 GMP 调度模型
- **Channel 通信**：协程间安全通信
- **工作窃取**：无锁本地队列优化

### 内存管理
- **自动 GC**：自动垃圾回收，无需手动管理内存
- **栈与堆**：值类型在栈上，引用类型在堆上
- **协程栈**：2KB 初始栈，可增长到 1MB

## 📖 代码示例

### Hello World

```q
println("Hello, World!")
```

### 变量和类型

```q
// 变量声明
var name: string = "Alice"
var age = 25                    // 类型推导
const MAX_AGE: int = 100        // 常量

// 集合类型
var numbers = [1, 2, 3, 4, 5]   // int[]
var config = {
    "host": "localhost",
    "port": "8080"
}  // map[string]string
```

### 类和对象

```q
class Person {
    func init(var name: string, var age: int) {}
    
    func greet() {
        println("Hello, I am " + this.name)
    }
}

var person = new Person("Alice", 25)
person.greet()
```

### 继承和多态

```q
class Animal {
    func init(var name: string) {}
    
    func speak() string {
        return "..."
    }
}

class Dog extends Animal {
    override func speak() string {
        return "Woof!"
    }
}

var dog = new Dog("Buddy")
println(dog.speak())  // 输出：Woof!
```

### Trait 使用

```q
trait Printable {
    func format() string
    
    func print() {
        println(this.format())
    }
}

class User {
    func init(var name: string, var age: int) {}
    
    use Printable
    
    func format() string {
        return "User(" + this.name + ", " + this.age as string + ")"
    }
}

var user = new User("Alice", 25)
user.print()  // 输出：User(Alice, 25)
```

### 并发编程

```q
// 启动协程
go func() {
    println("Hello from coroutine")
}()

// 并发计算
var compute = func(id: int) {
    for var i = 0; i < 5; i++ {
        println("Task " + id as string + ": " + i as string)
    }
}

go compute(1)
go compute(2)
```

### 错误处理

```q
var divide = func(a: int, b: int) int {
    if b == 0 {
        throw "ArithmeticException: Division by zero"
    }
    return a / b
}

try {
    var result = divide(10, 0)
    println(result)
} catch(e) {
    println("Error: " + e)
} finally {
    println("Cleanup completed")
}
```

## 🎯 学习建议

1. **动手实践**：每学习一个概念，都要编写代码实践
2. **阅读示例**：每个文档都包含完整示例，建议仔细阅读
3. **理解原理**：不仅要知道如何使用，还要理解背后的原理
4. **参考测试**：项目的 `tests/samples/` 目录包含许多测试用例

## 🔗 相关资源

- **项目仓库**：查看 Q 语言的源代码实现
- **测试用例**：`tests/samples/` 目录包含各种示例程序
- **语法设计**：`example/docs/语法设计.md` 是最初的语法设计文档

## ⚠️ 注意事项

本教程基于 Q 语言当前的实现状态编写：

- ✅ **已实现**：基本类型、控制结构、结构体、类、继承、接口、抽象类
- 🚧 **部分实现**：某些高级特性（如字符串方法、切片方法）可能尚未完全实现
- 📝 **设计中**：泛型、Trait、枚举等特性在设计文档中定义，但可能尚未实现

在使用时，请以实际编译器的行为为准。如果遇到与文档不符的情况，可能是该特性尚未完全实现。

## 📊 文档统计

| 类别 | 文档数量 | 覆盖内容 |
|------|---------|---------|
| 基础篇 | 4 | 变量、类型、运算符、控制结构 |
| 数据结构篇 | 2 | 结构体、面向对象 |
| 进阶篇 | 4 | 类型系统、函数、泛型、Trait |
| 实战篇 | 2 | 错误处理、并发编程 |
| **总计** | **12** | **完整覆盖 Q 语言核心特性** |

## 📝 文档版本

- **版本**：v2.0
- **更新日期**：2026-01-20
- **编写基于**：Q 语言编译器当前实现状态
- **文档数量**：12 篇核心教程
- **总字数**：约 50,000+ 字

## 🎓 学习成果

完成本教程后，你将能够：

- ✅ 掌握 Q 语言的类型系统和语法
- ✅ 编写面向对象的 Q 语言程序
- ✅ 使用泛型和 Trait 进行抽象编程
- ✅ 实现健壮的错误处理
- ✅ 编写高性能的并发程序
- ✅ 理解 Q 语言的内存模型和性能特性

---

开始你的 Q 语言学习之旅吧！

**推荐起点**：
- 🔰 初学者：从 [变量和常量](./变量和常量.md) 开始
- 🚀 有经验开发者：直接跳到 [面向对象](./面向对象.md)

祝学习愉快！ 🎉
