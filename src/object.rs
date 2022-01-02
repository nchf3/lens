use std::path::Path;
use tobj::*;

pub struct Object {
    pub models: Vec<Model>,
    pub textures: Option<Vec<(image::DynamicImage, String, String)>>,
}

impl Object {
    pub fn load_from<P: AsRef<Path>>(path: P) -> Object {
        let (obj_models, obj_materials) = tobj::load_obj(
            path.as_ref(),
            &LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
        )
        .unwrap();

        let obj_materials = obj_materials.unwrap();
        // We're assuming that the texture files are stored with the obj file
        let containing_folder = path.as_ref().parent().unwrap();

        let mut textures: Vec<(image::DynamicImage, String, String)> = Vec::new();
        for mat in obj_materials.clone() {
            let diffuse_path = mat.diffuse_texture;
            let path = containing_folder.join(diffuse_path.clone());
            let img = image::open(path).unwrap();
            let name = mat.name;

            textures.push((img, diffuse_path, name));
        }

        Object {
            models: obj_models,
            textures: Some(textures),
        }
    }
}
