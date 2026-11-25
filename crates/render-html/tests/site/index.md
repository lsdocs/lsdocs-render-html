# Test site {#test-site}

A page with the markup render-html writes: *emphasis*, `code`, a
[link](guide/setup.md) and a footnote[^1].

> [!NOTE]
> An admonition, with a title from its kind.

## Code {#code}

```rust
fn main() {
    println!("hello");
}
```

```toml title="lsdocs.toml" hl_lines="2"
[project]
name = "test"
```

### Deeper {#deeper}

| Name | Value |
|---|---|
| a | 1 |

[^1]: The footnote.
