# Lens

A 3D renderer front-end for WebGPU

# Summary

Lens is a project to play with webgpu api and try to understand how complex 3D renderers work.

# How to use it

Create a new scene :

```rust
let mut lens_scene = lens::Lens::new();
```

Load object files from "res" folder :

```rust
let res_dir = std::path::Path::new(env!("OUT_DIR")).join("res");
let cube_object = lens::Object::load_from(res_dir.join("cube").join("cube.obj"));
```

Link objects to the scene with associated shader file :

```rust
lens_scene.add_object(lens::LensObject {
    object: cube_object,
    position: cgmath::Vector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    },
    transform: None,
    shader_file: include_str!("../shader/shader.wgsl").into(),
    instances: None,
});
```

Once all is linked, run the scene :

```rust
lens_scene.run();
```