# Weekly notes

Plain paragraph with **bold**, *italic*, _underscored italic_, ***both***,
~~struck~~ and `inline code`. A [link](https://example.com "Title"), a
reference [link][ref], an autolink <https://example.com>, a bare
www.example.com and an escaped \*not emphasis\*.

[ref]: https://example.com/ref

## Lists

- First bullet
- Second bullet with **bold**
  - Nested bullet
    - Deeper, with `code`
- Back out

1. Ordered one
2. Ordered two
   continued lazily
10. Ordered ten

* [ ] Open task
* [x] Done task
+ Plus bullet

### Quotes

> A quote with *emphasis*
> > and a nested one
lazy continuation line

#### Code

```rust
fn main() {

    println!("blank line above");
}
```

    indented code block
    second line

##### Table

| Left | Centre | Right |
| :--- | :----: | ----: |
| a    | **b**  | `c`   |
| d    | e      | f     |

###### Rules and breaks

---
***
Line with a hard break  
next line, then a backslash break\
last line.

Setext heading
==============

Second setext
-------------

![An image](https://example.com/picture.png) and <span>inline HTML</span>.

<div>
An HTML block
</div>
