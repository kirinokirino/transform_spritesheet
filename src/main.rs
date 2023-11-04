use std::io::Write;

use anyhow::Result;
use glam::{Mat2, Vec2};
use image::io::Reader as ImageReader;
use image::{Pixel, Rgba, RgbaImage};

const PALETTE_SIZE: u8 = 64;
const TEXTURE_SRC_SIZE: u8 = 16;
const BLOCKS: u8 = 23;
const TEXTURE_WIDTH: usize = TEXTURE_SRC_SIZE as usize * BLOCKS as usize;
const SIDES: u8 = 3;
const TEXTURE_HEIGHT: usize = TEXTURE_SRC_SIZE as usize * SIDES as usize;

const ISOMETRIC_WIDTH: usize = (TEXTURE_SRC_SIZE * 2) as usize;
const ISOMETRIC_HEIGHT: usize = (TEXTURE_SRC_SIZE * 2) as usize;

pub struct TextureConverter {
    texture_map_path: String,
    img: RgbaImage,

    palette: Vec<Rgba<u8>>,
    unmodified_texture: Vec<Vec<Rgba<u8>>>,
    isometric_texture: Vec<Vec<Rgba<u8>>>,
}

impl TextureConverter {
    pub fn new(texture_map_path: String) -> Result<Self> {
        let img = ImageReader::open(&texture_map_path)?.decode()?.into_rgba8();
        let palette = Vec::new();
        let empty_face = vec![
            Rgba::<u8>([255, 0, 255, 255]);
            usize::from(TEXTURE_SRC_SIZE) * usize::from(TEXTURE_SRC_SIZE)
        ];
        let unmodified_texture = vec![empty_face; usize::from(BLOCKS) * usize::from(SIDES)];

        let empty_face_isometric = vec![Rgba::<u8>([255, 0, 255, 255]); 32 * 31];
        let isometric_texture =
            vec![empty_face_isometric; usize::from(BLOCKS) * usize::from(SIDES)];

        Ok(Self {
            texture_map_path,
            img,
            palette,
            unmodified_texture,
            isometric_texture,
        })
    }

    pub fn generate_rust_texture_source(&mut self) {
        let (width, height) = self.img.dimensions();
        self.normalize_transparent_pixels();
        self.fill_palette();
        self.set_unmodified_texture_source();
        self.set_isometric_texture_source();

        self.debug_draw();

        self.save_textures("./src/textures.rs");
    }

    fn save_textures(&self, path: &str) {
        let mut file = std::fs::File::options()
            .create(true)
            .write(true)
            .open(path)
            .unwrap();

        let size = ISOMETRIC_HEIGHT * ISOMETRIC_WIDTH;
        let mut contents = format!("pub const TEXTURES: [[[u8; 4]; {size}]; {BLOCKS}] = [\n");
        for texture in &self.isometric_texture {
            contents = format!("{contents}[\n");
            for pixel in texture {
                let [r, g, b, a] = pixel.channels() else {
                    panic!();
                };
                contents = format!("{contents}[{r}, {g}, {b}, {a}],");
            }
            contents = format!("{contents}],\n");
            file.write_all(contents.as_bytes());
            contents.clear();
        }
        contents = format!("{contents}];");
        file.write_all(contents.as_bytes());
        // println!("{}", contents);
    }

    fn set_unmodified_texture_source(&mut self) {
        for row in 0..TEXTURE_WIDTH {
            for col in 0..TEXTURE_HEIGHT {
                let block_x = row / TEXTURE_SRC_SIZE as usize;
                let block_y = col / TEXTURE_SRC_SIZE as usize;
                let x = row % TEXTURE_SRC_SIZE as usize;
                let y = col % TEXTURE_SRC_SIZE as usize;
                self.unmodified_texture[block_y + (block_x * usize::from(SIDES))]
                    [x + y * usize::from(TEXTURE_SRC_SIZE)] = *self
                    .palette
                    .iter()
                    .find(|e| {
                        e == &self
                            .img
                            .get_pixel(row.try_into().unwrap(), col.try_into().unwrap())
                    })
                    .unwrap();
            }
        }
    }

    fn set_isometric_texture_source(&mut self) {
        for (i, block_texture) in self
            .unmodified_texture
            .as_slice()
            .chunks_exact(3)
            .enumerate()
        {
            let top = &block_texture[0];
            let left = &block_texture[1];
            let right = &block_texture[2];

            let isometric_block_texture =
                self.transform_isometric_texture(top.as_slice(), left.as_slice(), right.as_slice());
            self.isometric_texture[i] = isometric_block_texture;
        }
    }

    // This function leaves 1 row of blank pixels at the top, todo?
    fn transform_isometric_texture(
        &self,
        top: &[Rgba<u8>],
        left: &[Rgba<u8>],
        right: &[Rgba<u8>],
    ) -> Vec<Rgba<u8>> {
        let mut out: Vec<Vec<_>> =
            vec![vec![Rgba::from([0, 0, 0, 0]); ISOMETRIC_WIDTH]; ISOMETRIC_HEIGHT];

        let top_offset = ISOMETRIC_HEIGHT / 4;
        // top side, transformation matrix, y offset is top_offset
        let transformation_matrix: Mat2 = Mat2::from_cols_array_2d(&[[1.0, -0.5], [1.0, 0.5]]);
        for y in 0..=ISOMETRIC_HEIGHT {
            for x in 0..=ISOMETRIC_WIDTH {
                let pos = Vec2::new(x as f32, y as f32 - top_offset as f32);
                let sample_pos = transformation_matrix.inverse().mul_vec2(pos);
                if fits_inside_rect(&sample_pos, TEXTURE_SRC_SIZE as f32) {
                    let sx = sample_pos[0].floor();
                    let sy = sample_pos[1].floor();
                    let idx = sx as usize + sy as usize * usize::from(TEXTURE_SRC_SIZE);
                    out[y][x] = top[idx];
                }
            }
        }
        // left, shear matrix, y offset is top_offset
        let shear = Vec2::new(-0.5, 0.0);
        let shear_matrix = Mat2::from_cols_array_2d(&[[1.0, shear.x], [shear.y, 1.0]]);
        for y in 0..=ISOMETRIC_HEIGHT {
            for x in 0..=ISOMETRIC_WIDTH {
                let pos = Vec2::new(x as f32, y as f32 - top_offset as f32);
                let sample_pos = shear_matrix.mul_vec2(pos);
                if fits_inside_rect(&sample_pos, TEXTURE_SRC_SIZE as f32) {
                    let sx = sample_pos[0].floor();
                    let sy = sample_pos[1].floor();
                    let idx = sx as usize + sy as usize * usize::from(TEXTURE_SRC_SIZE);
                    out[y][x] = left[idx];
                }
            }
        }

        // Right, shear matrix, center is offset
        let center = ISOMETRIC_HEIGHT as f32 / 2.0;
        let shear = Vec2::new(0.5, 0.0);
        let shear_matrix = Mat2::from_cols_array_2d(&[[1.0, shear.x], [shear.y, 1.0]]);
        for y in 0..=ISOMETRIC_HEIGHT {
            for x in 0..=ISOMETRIC_WIDTH {
                let pos = Vec2::new(x as f32 - center, y as f32 - center);
                let sample_pos = shear_matrix.mul_vec2(pos);
                if fits_inside_rect(&sample_pos, TEXTURE_SRC_SIZE as f32) {
                    let sx = sample_pos[0].floor();
                    let sy = sample_pos[1].floor();
                    let idx = sx as usize + sy as usize * usize::from(TEXTURE_SRC_SIZE);
                    out[y][x] = right[idx];
                }
            }
        }
        out.concat()
    }

    fn fill_palette(&mut self) {
        for pixel in self.img.pixels() {
            if !self.palette.contains(pixel) {
                self.palette.push(*pixel);
            }
        }
        let palette = self.palette.clone();
        // water
        for pixel in palette {
            let color = pixel.channels();
            let slice = [color[0] / 2, color[1] / 2, 255 / 2, color[3]];
            let water = Rgba::from_slice(&slice);
            self.palette.push(*water);
        }
        let palette = self.palette.clone();
        // shadow
        for pixel in palette {
            let color = pixel.channels();
            let slice = [color[0] / 2, color[1] / 2, color[2] / 2, color[3]];
            let water = Rgba::from_slice(&slice);
            self.palette.push(*water);
        }
    }

    // Force transparent pixels to the same color
    fn normalize_transparent_pixels(&mut self) {
        for pixel in self.img.pixels_mut() {
            let p = pixel.channels_mut();
            if p[3] == 0 {
                p[0] = 0;
                p[1] = 0;
                p[2] = 0;
            }
        }
    }

    fn debug_draw(&self) {
        let mut buffer: Vec<u8> = Vec::with_capacity(640 * 480 * 4);
        unsafe { buffer.set_len(buffer.capacity()) };
        buffer.fill(0);
        for (texture_idx, texture) in self.unmodified_texture.iter().enumerate() {
            let side = texture_idx % SIDES as usize;
            let block = texture_idx / SIDES as usize;
            for (i, pixel) in texture.iter().enumerate() {
                let block_y = i / TEXTURE_SRC_SIZE as usize;
                let block_x = i % TEXTURE_SRC_SIZE as usize;
                let block_offset =
                    (side * TEXTURE_SRC_SIZE as usize) * 640 + (block * TEXTURE_SRC_SIZE as usize);
                let buffer_offset = block_offset + block_x + block_y * 640;
                let [r, g, b, a] = pixel.0;
                buffer[buffer_offset * 4] = r;
                buffer[buffer_offset * 4 + 1] = g;
                buffer[buffer_offset * 4 + 2] = b;
                buffer[buffer_offset * 4 + 3] = a;
            }
        }
        let y_off = 640 * (TEXTURE_SRC_SIZE * 3) as usize;
        for (texture_idx, texture) in self.isometric_texture.iter().take(15).enumerate() {
            let x_off = ISOMETRIC_WIDTH * texture_idx;
            for (i, pixel) in texture.iter().enumerate() {
                let [r, g, b, a] = pixel.0;
                let x = i % ISOMETRIC_WIDTH;
                let y = i / ISOMETRIC_WIDTH;
                let block_offset = y_off + x_off + x + y * 640;
                buffer[block_offset * 4] = r;
                buffer[block_offset * 4 + 1] = g;
                buffer[block_offset * 4 + 2] = b;
                buffer[block_offset * 4 + 3] = a;
            }
        }

        use std::io::Write;
        let file = std::fs::File::options()
            .create(true)
            .read(true)
            .write(true)
            .open("/tmp/imagesink")
            .unwrap();
        let size = 640 * 480 * 4;
        file.set_len(size.try_into().unwrap()).unwrap();
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file).unwrap() };
        if let Some(err) = mmap.lock().err() {
            panic!("{err}");
        }
        let _ = (&mut mmap[..]).write_all(&buffer);
    }
}

pub fn fits_inside_rect(v: &Vec2, rect_size: f32) -> bool {
    // todo epsilon or something?
    v.x < rect_size && v.y < rect_size && v.x >= 0.0 && v.y >= 0.0
}
