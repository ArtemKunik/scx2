import pathlib

p = pathlib.Path("src/ui.rs")
s = p.read_text(encoding="utf-8")
s = s.replace("            .with_inner_size([1440.0, 900.0])\n", "")
s = s.replace("            .with_maximized(true)\n", "")
p.write_text(s, encoding="utf-8", newline="\n")
print("size overrides removed")
