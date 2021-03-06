use std::ptr;
use std::sync::mpsc::Sender;
use std::collections::HashMap;
use std::default::Default;

use BufferExt;
use ProgramExt;
use DrawError;
use Handle;
use RawUniformValue;

use context::Context;
use ContextExt;

use utils::bitsfield::Bitsfield;

use fbo::{self, FramebufferAttachments};

use sync;
use uniforms::{Uniforms, UniformValue, SamplerBehavior};
use sampler_object::SamplerObject;
use {Program, GlObject, ToGlEnum};
use index::{self, IndicesSource};
use vertex::{MultiVerticesSource, VerticesSource};

use draw_parameters::DrawParameters;
use draw_parameters::{BlendingFunction, BackfaceCullingMode};
use draw_parameters::{DepthTest, PolygonMode};
use draw_parameters::{StencilTest};
use Rect;

use program;
use libc;
use {gl, context, draw_parameters};
use version::Version;
use version::Api;

/// Draws everything.
pub fn draw<'a, I, U, V>(context: &Context, framebuffer: Option<&FramebufferAttachments>,
                         vertex_buffers: V, mut indices: IndicesSource<I>,
                         program: &Program, uniforms: &U, draw_parameters: &DrawParameters,
                         dimensions: (u32, u32)) -> Result<(), DrawError>
                         where U: Uniforms, I: index::Index, V: MultiVerticesSource<'a>
{
    try!(draw_parameters::validate(context, draw_parameters));

    // this contains the list of fences that will need to be fulfilled after the draw command
    // has started
    let mut fences = Vec::with_capacity(0);

    // handling tessellation
    let vertices_per_patch = match indices.get_primitives_type() {
        index::PrimitiveType::Patches { vertices_per_patch } => {
            if let Some(max) = context.capabilities().max_patch_vertices {
                if vertices_per_patch == 0 || vertices_per_patch as gl::types::GLint > max {
                    return Err(DrawError::UnsupportedVerticesPerPatch);
                }
            } else {
                return Err(DrawError::TessellationNotSupported);
            }

            // TODO: programs created from binaries have the wrong value
            // for `has_tessellation_shaders`
            /*if !program.has_tessellation_shaders() {    // TODO: 
                panic!("Default tessellation level is not supported yet");
            }*/

            Some(vertices_per_patch)
        },
        _ => {
            // TODO: programs created from binaries have the wrong value
            // for `has_tessellation_shaders`
            /*if program.has_tessellation_shaders() {
                return Err(DrawError::TessellationWithoutPatches);
            }*/

            None
        },
    };

    // starting the state changes
    let mut ctxt = context.make_current();

    // handling vertices source
    let (vertices_count, instances_count) = {
        let ib_id = match indices {
            IndicesSource::Buffer { .. } => 0,
            IndicesSource::IndexBuffer { ref buffer, .. } => buffer.get_id(),
            IndicesSource::NoIndices { .. } => 0,
        };

        // object that is used to build the bindings
        let mut binder = context.vertex_array_objects.start(&mut ctxt, program, ib_id);
        // number of vertices in the vertices sources, or `None` if there is a mismatch
        let mut vertices_count: Option<usize> = None;
        // number of instances to draw
        let mut instances_count: Option<usize> = None;

        for src in vertex_buffers.iter() {
            match src {
                VerticesSource::VertexBuffer(ref buffer, offset, _, per_instance) => {
                    if let Some(fence) = buffer.add_fence() {
                        fences.push(fence);
                    }

                    binder = binder.add(buffer, offset, if per_instance { Some(1) } else { None });
                },
                _ => {}
            }

            match src {
                VerticesSource::VertexBuffer(_, _, len, false) => {
                    if let Some(curr) = vertices_count {
                        if curr != len {
                            vertices_count = None;
                            break;
                        }
                    } else {
                        vertices_count = Some(len);
                    }
                },
                VerticesSource::VertexBuffer(_, _, len, true) => {
                    if let Some(curr) = instances_count {
                        if curr != len {
                            return Err(DrawError::InstancesCountMismatch);
                        }
                    } else {
                        instances_count = Some(len);
                    }
                },
                VerticesSource::Marker { len, per_instance } if !per_instance => {
                    if let Some(curr) = vertices_count {
                        if curr != len {
                            vertices_count = None;
                            break;
                        }
                    } else {
                        vertices_count = Some(len);
                    }
                },
                VerticesSource::Marker { len, per_instance } if per_instance => {
                    if let Some(curr) = instances_count {
                        if curr != len {
                            return Err(DrawError::InstancesCountMismatch);
                        }
                    } else {
                        instances_count = Some(len);
                    }
                },
                _ => ()
            }
        }

        binder.bind();

        (vertices_count, instances_count)
    };

    // binding the FBO to draw upon
    {
        let fbo_id = context.framebuffer_objects.as_ref().unwrap()
                            .get_framebuffer_for_drawing(framebuffer, &mut ctxt);
        fbo::bind_framebuffer(&mut ctxt, fbo_id, true, false);
    };

    // binding the program
    unsafe {
        let program_id = program.get_id();
        if ctxt.state.program != program_id {
            match program_id {
                Handle::Id(id) => ctxt.gl.UseProgram(id),
                Handle::Handle(id) => ctxt.gl.UseProgramObjectARB(id),
            }
            ctxt.state.program = program_id;
        }
    }

    // building the list of uniforms binders and the fences that must be fulfilled
    // TODO: panic if uniforms of the program are not found in the parameter
    {
        let mut texture_bind_points = Bitsfield::new();
        let mut buffer_bind_points = Bitsfield::new();

        let mut visiting_result = Ok(());
        uniforms.visit_values(|name, value| {
            if visiting_result.is_err() { return; }

            if let Some(uniform) = program.get_uniform(name) {
                assert!(uniform.size.is_none(), "Uniform arrays not supported yet");

                if !value.is_usable_with(&uniform.ty) {
                    visiting_result = Err(DrawError::UniformTypeMismatch {
                        name: name.to_string(),
                        expected: uniform.ty,
                    });
                    return;
                }

                match bind_uniform(&mut ctxt, &mut context.samplers.borrow_mut(),
                                   &value, program, uniform.location,
                                   &mut texture_bind_points, name)
                {
                    Ok(_) => (),
                    Err(e) => {
                        visiting_result = Err(e);
                        return;
                    }
                };

            } else if let Some(block) = program.get_uniform_blocks().get(name) {
                let fence = match bind_uniform_block(&mut ctxt, &value, block,
                                                     program, &mut buffer_bind_points, name)
                {
                    Ok(f) => f,
                    Err(e) => {
                        visiting_result = Err(e);
                        return;
                    }
                };

                if let Some(fence) = fence {
                    fences.push(fence);
                }
            }
        });

        if let Err(e) = visiting_result {
            return Err(e);
        }
    }

    // sync-ing draw_parameters
    unsafe {
        sync_depth(&mut ctxt, draw_parameters.depth_test, draw_parameters.depth_write,
                   draw_parameters.depth_range);
        sync_stencil(&mut ctxt, &draw_parameters);
        sync_blending(&mut ctxt, draw_parameters.blending_function);
        sync_line_width(&mut ctxt, draw_parameters.line_width);
        sync_point_size(&mut ctxt, draw_parameters.point_size);
        sync_polygon_mode(&mut ctxt, draw_parameters.backface_culling, draw_parameters.polygon_mode);
        sync_multisampling(&mut ctxt, draw_parameters.multisampling);
        sync_dithering(&mut ctxt, draw_parameters.dithering);
        sync_viewport_scissor(&mut ctxt, draw_parameters.viewport, draw_parameters.scissor,
                              dimensions);
        sync_rasterizer_discard(&mut ctxt, draw_parameters.draw_primitives);
        sync_vertices_per_patch(&mut ctxt, vertices_per_patch);

        if !program.has_srgb_output() {
            if ctxt.version >= &Version(Api::Gl, 3, 0) || ctxt.extensions.gl_arb_framebuffer_srgb ||
               ctxt.extensions.gl_ext_framebuffer_srgb
            {
                if !ctxt.state.enabled_framebuffer_srgb {
                    ctxt.gl.Enable(gl::FRAMEBUFFER_SRGB);
                    ctxt.state.enabled_framebuffer_srgb = true;
                }
            }
        }
    }

    // drawing
    {
        match &indices {
            &IndicesSource::IndexBuffer { ref buffer, offset, length, .. } => {
                let ptr: *const u8 = ptr::null_mut();
                let ptr = unsafe { ptr.offset((offset * buffer.get_indices_type().get_size()) as isize) };

                if let Some(fence) = buffer.add_fence() {
                    fences.push(fence);
                }

                unsafe {
                    if let Some(instances_count) = instances_count {
                        ctxt.gl.DrawElementsInstanced(buffer.get_primitives_type().to_glenum(),
                                                      length as gl::types::GLsizei,
                                                      buffer.get_indices_type().to_glenum(),
                                                      ptr as *const libc::c_void,
                                                      instances_count as gl::types::GLsizei);
                    } else {
                        ctxt.gl.DrawElements(buffer.get_primitives_type().to_glenum(),
                                             length as gl::types::GLsizei,
                                             buffer.get_indices_type().to_glenum(),
                                             ptr as *const libc::c_void);
                    }
                }
            },

            &IndicesSource::Buffer { ref pointer, primitives, offset, length } => {
                assert!(offset == 0);       // not yet implemented

                unsafe {
                    if let Some(instances_count) = instances_count {
                        ctxt.gl.DrawElementsInstanced(primitives.to_glenum(),
                                                      length as gl::types::GLsizei,
                                                      <I as index::Index>::get_type().to_glenum(),
                                                      pointer.as_ptr() as *const gl::types::GLvoid,
                                                      instances_count as gl::types::GLsizei);
                    } else {
                        ctxt.gl.DrawElements(primitives.to_glenum(), length as gl::types::GLsizei,
                                             <I as index::Index>::get_type().to_glenum(),
                                             pointer.as_ptr() as *const gl::types::GLvoid);
                    }
                }
            },

            &IndicesSource::NoIndices { primitives } => {
                let vertices_count = match vertices_count {
                    Some(c) => c,
                    None => return Err(DrawError::VerticesSourcesLengthMismatch)
                };

                unsafe {
                    if let Some(instances_count) = instances_count {
                        ctxt.gl.DrawArraysInstanced(primitives.to_glenum(), 0,
                                                    vertices_count as gl::types::GLsizei,
                                                    instances_count as gl::types::GLsizei);
                    } else {
                        ctxt.gl.DrawArrays(primitives.to_glenum(), 0,
                                           vertices_count as gl::types::GLsizei);
                    }
                }
            },
        };
    };

    // fulfilling the fences
    for fence in fences.into_iter() {
        unsafe {
            fence.send(sync::new_linear_sync_fence_if_supported(&mut ctxt).unwrap()).unwrap();
        }
    }

    Ok(())
}

fn bind_uniform_block(ctxt: &mut context::CommandContext, value: &UniformValue,
                      block: &program::UniformBlock,
                      program: &Program, buffer_bind_points: &mut Bitsfield, name: &str)
                      -> Result<Option<Sender<sync::LinearSyncFence>>, DrawError>
{
    match value {
        &UniformValue::Block(ref buffer, ref layout) => {
            if !layout(block) {
                return Err(DrawError::UniformBlockLayoutMismatch { name: name.to_string() });
            }

            let bind_point = buffer_bind_points.get_unused().expect("Not enough buffer units");
            buffer_bind_points.set_used(bind_point);

            let fence = buffer.add_fence();
            let buffer = buffer.get_id();
            let binding = block.binding as gl::types::GLuint;

            unsafe {
                ctxt.gl.BindBufferBase(gl::UNIFORM_BUFFER, bind_point as gl::types::GLuint,
                                       buffer);
                program.set_block(ctxt, binding, bind_point as gl::types::GLuint);
            }

            Ok(fence)
        },
        _ => {
            Err(DrawError::UniformValueToBlock { name: name.to_string() })
        }
    }
}

fn bind_uniform(ctxt: &mut context::CommandContext,
                samplers: &mut HashMap<SamplerBehavior, SamplerObject>,
                value: &UniformValue, program: &Program, location: gl::types::GLint,
                texture_bind_points: &mut Bitsfield, name: &str)
                -> Result<(), DrawError>
{
    assert!(location >= 0);

    match *value {
        UniformValue::Block(_, _) => {
            Err(DrawError::UniformBufferToValue {
                name: name.to_string(),
            })
        },
        UniformValue::SignedInt(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::SignedInt(val));
            Ok(())
        },
        UniformValue::UnsignedInt(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::UnsignedInt(val));
            Ok(())
        },
        UniformValue::Float(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Float(val));
            Ok(())
        },
        UniformValue::Mat2(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Mat2(val));
            Ok(())
        },
        UniformValue::Mat3(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Mat3(val));
            Ok(())
        },
        UniformValue::Mat4(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Mat4(val));
            Ok(())
        },
        UniformValue::Vec2(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Vec2(val));
            Ok(())
        },
        UniformValue::Vec3(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Vec3(val));
            Ok(())
        },
        UniformValue::Vec4(val) => {
            program.set_uniform(ctxt, location, &RawUniformValue::Vec4(val));
            Ok(())
        },
        UniformValue::Texture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::CompressedTexture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::SrgbTexture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::CompressedSrgbTexture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::IntegralTexture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::UnsignedTexture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::DepthTexture1d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D)
        },
        UniformValue::Texture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::CompressedTexture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::SrgbTexture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::CompressedSrgbTexture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::IntegralTexture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::UnsignedTexture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::DepthTexture2d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D)
        },
        UniformValue::Texture2dMultisample(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE)
        },
        UniformValue::SrgbTexture2dMultisample(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE)
        },
        UniformValue::IntegralTexture2dMultisample(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE)
        },
        UniformValue::UnsignedTexture2dMultisample(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE)
        },
        UniformValue::DepthTexture2dMultisample(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE)
        },
        UniformValue::Texture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::CompressedTexture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::SrgbTexture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::CompressedSrgbTexture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::IntegralTexture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::UnsignedTexture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::DepthTexture3d(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_3D)
        },
        UniformValue::Texture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::CompressedTexture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::SrgbTexture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::CompressedSrgbTexture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::IntegralTexture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::UnsignedTexture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::DepthTexture1dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_1D_ARRAY)
        },
        UniformValue::Texture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::CompressedTexture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::SrgbTexture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::CompressedSrgbTexture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::IntegralTexture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::UnsignedTexture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::DepthTexture2dArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_ARRAY)
        },
        UniformValue::Texture2dMultisampleArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE_ARRAY)
        },
        UniformValue::SrgbTexture2dMultisampleArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE_ARRAY)
        },
        UniformValue::IntegralTexture2dMultisampleArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE_ARRAY)
        },
        UniformValue::UnsignedTexture2dMultisampleArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE_ARRAY)
        },
        UniformValue::DepthTexture2dMultisampleArray(texture, sampler) => {
            let texture = texture.get_id();
            bind_texture_uniform(ctxt, samplers, texture, sampler, location, program, texture_bind_points, gl::TEXTURE_2D_MULTISAMPLE_ARRAY)
        },
    }
}

fn bind_texture_uniform(mut ctxt: &mut context::CommandContext,
                        samplers: &mut HashMap<SamplerBehavior, SamplerObject>,
                        texture: gl::types::GLuint,
                        sampler: Option<SamplerBehavior>, location: gl::types::GLint,
                        program: &Program,
                        texture_bind_points: &mut Bitsfield,
                        bind_point: gl::types::GLenum)
                        -> Result<(), DrawError>
{
    let sampler = if let Some(sampler) = sampler {
        Some(try!(::sampler_object::get_sampler(ctxt, samplers, &sampler)))
    } else {
        None
    };

    let sampler = sampler.unwrap_or(0);

    // finding an appropriate texture unit
    let texture_unit =
        ctxt.state.texture_units
            .iter().enumerate()
            .find(|&(unit, content)| {
                content.texture == texture && (content.sampler == sampler ||
                                               !texture_bind_points.is_used(unit as u16))
            })
            .map(|(unit, _)| unit as u16)
            .or_else(|| {
                if ctxt.state.texture_units.len() <
                    ctxt.capabilities.max_combined_texture_image_units as usize
                {
                    Some(ctxt.state.texture_units.len() as u16)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                texture_bind_points.get_unused().expect("Not enough texture units available")
            });
    assert!((texture_unit as gl::types::GLint) <
            ctxt.capabilities.max_combined_texture_image_units);
    texture_bind_points.set_used(texture_unit);

    // updating the program to use the right unit
    program.set_uniform(ctxt, location,
                        &RawUniformValue::SignedInt(texture_unit as gl::types::GLint));

    // updating the state of the texture unit
    if ctxt.state.texture_units.len() <= texture_unit as usize {
        for _ in (ctxt.state.texture_units.len() .. texture_unit as usize + 1) {
            ctxt.state.texture_units.push(Default::default());
        }
    }

    if ctxt.state.texture_units[texture_unit as usize].texture != texture ||
       ctxt.state.texture_units[texture_unit as usize].sampler != sampler
    {
        // TODO: what if it's not supported?
        if ctxt.state.active_texture != texture_unit as gl::types::GLenum {
            unsafe { ctxt.gl.ActiveTexture(texture_unit as gl::types::GLenum + gl::TEXTURE0) };
            ctxt.state.active_texture = texture_unit as gl::types::GLenum;
        }

        if ctxt.state.texture_units[texture_unit as usize].texture != texture {
            unsafe { ctxt.gl.BindTexture(bind_point, texture); }
            ctxt.state.texture_units[texture_unit as usize].texture = texture;
        }

        if ctxt.state.texture_units[texture_unit as usize].sampler != sampler {
            assert!(ctxt.version >= &Version(Api::Gl, 3, 3) ||
                    ctxt.extensions.gl_arb_sampler_objects);

            unsafe { ctxt.gl.BindSampler(texture_unit as gl::types::GLenum, sampler); }
            ctxt.state.texture_units[texture_unit as usize].sampler = sampler;
        }
    }

    Ok(())
}

fn sync_depth(ctxt: &mut context::CommandContext, depth_test: DepthTest, depth_write: bool,
              depth_range: (f32, f32))
{
    // depth test
    match depth_test {
        DepthTest::Overwrite => unsafe {
            if ctxt.state.enabled_depth_test {
                ctxt.gl.Disable(gl::DEPTH_TEST);
                ctxt.state.enabled_depth_test = false;
            }
        },
        depth_function => unsafe {
            let depth_function = depth_function.to_glenum();
            if ctxt.state.depth_func != depth_function {
                ctxt.gl.DepthFunc(depth_function);
                ctxt.state.depth_func = depth_function;
            }
            if !ctxt.state.enabled_depth_test {
                ctxt.gl.Enable(gl::DEPTH_TEST);
                ctxt.state.enabled_depth_test = true;
            }
        }
    }

    // depth mask
    if depth_write != ctxt.state.depth_mask {
        unsafe {
            ctxt.gl.DepthMask(if depth_write { gl::TRUE } else { gl::FALSE });
        }
        ctxt.state.depth_mask = depth_write;
    }

    // depth range
    if depth_range != ctxt.state.depth_range {
        unsafe {
            ctxt.gl.DepthRange(depth_range.0 as f64, depth_range.1 as f64);
        }
        ctxt.state.depth_range = depth_range;
    }
}

fn sync_stencil(ctxt: &mut context::CommandContext, params: &DrawParameters) {
    // TODO: optimize me

    let (test_cw, read_mask_cw) = match params.stencil_test_clockwise {
        StencilTest::AlwaysPass => (gl::ALWAYS, 0),
        StencilTest::AlwaysFail => (gl::NEVER, 0),
        StencilTest::IfLess { mask } => (gl::LESS, mask),
        StencilTest::IfLessOrEqual { mask } => (gl::LEQUAL, mask),
        StencilTest::IfMore { mask } => (gl::GREATER, mask),
        StencilTest::IfMoreOrEqual { mask } => (gl::GEQUAL, mask),
        StencilTest::IfEqual { mask } => (gl::EQUAL, mask),
        StencilTest::IfNotEqual { mask } => (gl::NOTEQUAL, mask),
    };

    let (test_ccw, read_mask_ccw) = match params.stencil_test_counter_clockwise {
        StencilTest::AlwaysPass => (gl::ALWAYS, 0),
        StencilTest::AlwaysFail => (gl::NEVER, 0),
        StencilTest::IfLess { mask } => (gl::LESS, mask),
        StencilTest::IfLessOrEqual { mask } => (gl::LEQUAL, mask),
        StencilTest::IfMore { mask } => (gl::GREATER, mask),
        StencilTest::IfMoreOrEqual { mask } => (gl::GEQUAL, mask),
        StencilTest::IfEqual { mask } => (gl::EQUAL, mask),
        StencilTest::IfNotEqual { mask } => (gl::NOTEQUAL, mask),
    };

    if ctxt.state.stencil_func_back != (test_cw, params.stencil_reference_value_clockwise, read_mask_cw) {
        unsafe { ctxt.gl.StencilFuncSeparate(gl::BACK, test_cw, params.stencil_reference_value_clockwise, read_mask_cw) };
        ctxt.state.stencil_func_back = (test_cw, params.stencil_reference_value_clockwise, read_mask_cw);
    }

    if ctxt.state.stencil_func_front != (test_ccw, params.stencil_reference_value_counter_clockwise, read_mask_ccw) {
        unsafe { ctxt.gl.StencilFuncSeparate(gl::FRONT, test_cw, params.stencil_reference_value_clockwise, read_mask_cw) };
        ctxt.state.stencil_func_front = (test_ccw, params.stencil_reference_value_counter_clockwise, read_mask_ccw);
    }

    if ctxt.state.stencil_mask_back != params.stencil_write_mask_clockwise {
        unsafe { ctxt.gl.StencilMaskSeparate(gl::BACK, params.stencil_write_mask_clockwise) };
        ctxt.state.stencil_mask_back = params.stencil_write_mask_clockwise;
    }

    if ctxt.state.stencil_mask_front != params.stencil_write_mask_clockwise {
        unsafe { ctxt.gl.StencilMaskSeparate(gl::FRONT, params.stencil_write_mask_clockwise) };
        ctxt.state.stencil_mask_front = params.stencil_write_mask_clockwise;
    }

    let op_back = (params.stencil_fail_operation_clockwise.to_glenum(),
                   params.stencil_pass_depth_fail_operation_clockwise.to_glenum(),
                   params.stencil_depth_pass_operation_clockwise.to_glenum());
    if ctxt.state.stencil_op_back != op_back {
        unsafe { ctxt.gl.StencilOpSeparate(gl::BACK, op_back.0, op_back.1, op_back.2) };
        ctxt.state.stencil_op_back = op_back;
    }

    let op_front = (params.stencil_fail_operation_counter_clockwise.to_glenum(),
                    params.stencil_pass_depth_fail_operation_counter_clockwise.to_glenum(),
                    params.stencil_depth_pass_operation_counter_clockwise.to_glenum());
    if ctxt.state.stencil_op_front != op_front {
        unsafe { ctxt.gl.StencilOpSeparate(gl::FRONT, op_front.0, op_front.1, op_front.2) };
        ctxt.state.stencil_op_front = op_front;
    }

    let enable_stencil = test_cw != gl::ALWAYS || test_ccw != gl::ALWAYS ||
                         op_back.0 != gl::KEEP || op_front.0 != gl::KEEP;
    if ctxt.state.enabled_stencil_test != enable_stencil {
        if enable_stencil {
            unsafe { ctxt.gl.Enable(gl::STENCIL_TEST) };
        } else {
            unsafe { ctxt.gl.Disable(gl::STENCIL_TEST) };
        }

        ctxt.state.enabled_stencil_test = enable_stencil;
    }
}

fn sync_blending(ctxt: &mut context::CommandContext, blending_function: Option<BlendingFunction>) {
    let blend_factors = match blending_function {
        Some(BlendingFunction::AlwaysReplace) => unsafe {
            if ctxt.state.enabled_blend {
                ctxt.gl.Disable(gl::BLEND);
                ctxt.state.enabled_blend = false;
            }
            None
        },
        Some(BlendingFunction::Min) => unsafe {
            if ctxt.state.blend_equation != gl::MIN {
                ctxt.gl.BlendEquation(gl::MIN);
                ctxt.state.blend_equation = gl::MIN;
            }
            if !ctxt.state.enabled_blend {
                ctxt.gl.Enable(gl::BLEND);
                ctxt.state.enabled_blend = true;
            }
            None
        },
        Some(BlendingFunction::Max) => unsafe {
            if ctxt.state.blend_equation != gl::MAX {
                ctxt.gl.BlendEquation(gl::MAX);
                ctxt.state.blend_equation = gl::MAX;
            }
            if !ctxt.state.enabled_blend {
                ctxt.gl.Enable(gl::BLEND);
                ctxt.state.enabled_blend = true;
            }
            None
        },
        Some(BlendingFunction::Addition { source, destination }) => unsafe {
            if ctxt.state.blend_equation != gl::FUNC_ADD {
                ctxt.gl.BlendEquation(gl::FUNC_ADD);
                ctxt.state.blend_equation = gl::FUNC_ADD;
            }
            if !ctxt.state.enabled_blend {
                ctxt.gl.Enable(gl::BLEND);
                ctxt.state.enabled_blend = true;
            }
            Some((source, destination))
        },
        Some(BlendingFunction::Subtraction { source, destination }) => unsafe {
            if ctxt.state.blend_equation != gl::FUNC_SUBTRACT {
                ctxt.gl.BlendEquation(gl::FUNC_SUBTRACT);
                ctxt.state.blend_equation = gl::FUNC_SUBTRACT;
            }
            if !ctxt.state.enabled_blend {
                ctxt.gl.Enable(gl::BLEND);
                ctxt.state.enabled_blend = true;
            }
            Some((source, destination))
        },
        Some(BlendingFunction::ReverseSubtraction { source, destination }) => unsafe {
            if ctxt.state.blend_equation != gl::FUNC_REVERSE_SUBTRACT {
                ctxt.gl.BlendEquation(gl::FUNC_REVERSE_SUBTRACT);
                ctxt.state.blend_equation = gl::FUNC_REVERSE_SUBTRACT;
            }
            if !ctxt.state.enabled_blend {
                ctxt.gl.Enable(gl::BLEND);
                ctxt.state.enabled_blend = true;
            }
            Some((source, destination))
        },
        _ => None
    };
    if let Some((source, destination)) = blend_factors {
        let source = source.to_glenum();
        let destination = destination.to_glenum();

        if ctxt.state.blend_func != (source, destination) {
            unsafe { ctxt.gl.BlendFunc(source, destination) };
            ctxt.state.blend_func = (source, destination);
        }
    };
}

fn sync_line_width(ctxt: &mut context::CommandContext, line_width: Option<f32>) {
    if let Some(line_width) = line_width {
        if ctxt.state.line_width != line_width {
            unsafe {
                ctxt.gl.LineWidth(line_width);
                ctxt.state.line_width = line_width;
            }
        }
    }
}

fn sync_point_size(ctxt: &mut context::CommandContext, point_size: Option<f32>) {
    if let Some(point_size) = point_size {
        if ctxt.state.point_size != point_size {
            unsafe {
                ctxt.gl.PointSize(point_size);
                ctxt.state.point_size = point_size;
            }
        }
    }
}

fn sync_polygon_mode(ctxt: &mut context::CommandContext, backface_culling: BackfaceCullingMode,
                     polygon_mode: PolygonMode)
{
    // back-face culling
    // note: we never change the value of `glFrontFace`, whose default is GL_CCW
    //  that's why `CullClockWise` uses `GL_BACK` for example
    match backface_culling {
        BackfaceCullingMode::CullingDisabled => unsafe {
            if ctxt.state.enabled_cull_face {
                ctxt.gl.Disable(gl::CULL_FACE);
                ctxt.state.enabled_cull_face = false;
            }
        },
        BackfaceCullingMode::CullCounterClockWise => unsafe {
            if !ctxt.state.enabled_cull_face {
                ctxt.gl.Enable(gl::CULL_FACE);
                ctxt.state.enabled_cull_face = true;
            }
            if ctxt.state.cull_face != gl::FRONT {
                ctxt.gl.CullFace(gl::FRONT);
                ctxt.state.cull_face = gl::FRONT;
            }
        },
        BackfaceCullingMode::CullClockWise => unsafe {
            if !ctxt.state.enabled_cull_face {
                ctxt.gl.Enable(gl::CULL_FACE);
                ctxt.state.enabled_cull_face = true;
            }
            if ctxt.state.cull_face != gl::BACK {
                ctxt.gl.CullFace(gl::BACK);
                ctxt.state.cull_face = gl::BACK;
            }
        },
    }

    // polygon mode
    unsafe {
        let polygon_mode = polygon_mode.to_glenum();
        if ctxt.state.polygon_mode != polygon_mode {
            ctxt.gl.PolygonMode(gl::FRONT_AND_BACK, polygon_mode);
            ctxt.state.polygon_mode = polygon_mode;
        }
    }
}

fn sync_multisampling(ctxt: &mut context::CommandContext, multisampling: bool) {
    if ctxt.state.enabled_multisample != multisampling {
        unsafe {
            if multisampling {
                ctxt.gl.Enable(gl::MULTISAMPLE);
                ctxt.state.enabled_multisample = true;
            } else {
                ctxt.gl.Disable(gl::MULTISAMPLE);
                ctxt.state.enabled_multisample = false;
            }
        }
    }
}

fn sync_dithering(ctxt: &mut context::CommandContext, dithering: bool) {
    if ctxt.state.enabled_dither != dithering {
        unsafe {
            if dithering {
                ctxt.gl.Enable(gl::DITHER);
                ctxt.state.enabled_dither = true;
            } else {
                ctxt.gl.Disable(gl::DITHER);
                ctxt.state.enabled_dither = false;
            }
        }
    }
}

fn sync_viewport_scissor(ctxt: &mut context::CommandContext, viewport: Option<Rect>,
                         scissor: Option<Rect>, surface_dimensions: (u32, u32))
{
    // viewport
    if let Some(viewport) = viewport {
        assert!(viewport.width <= ctxt.capabilities.max_viewport_dims.0 as u32,
                "Viewport dimensions are too large");
        assert!(viewport.height <= ctxt.capabilities.max_viewport_dims.1 as u32,
                "Viewport dimensions are too large");

        let viewport = (viewport.left as gl::types::GLint, viewport.bottom as gl::types::GLint,
                        viewport.width as gl::types::GLsizei,
                        viewport.height as gl::types::GLsizei);

        if ctxt.state.viewport != Some(viewport) {
            unsafe { ctxt.gl.Viewport(viewport.0, viewport.1, viewport.2, viewport.3); }
            ctxt.state.viewport = Some(viewport);
        }

    } else {
        assert!(surface_dimensions.0 <= ctxt.capabilities.max_viewport_dims.0 as u32,
                "Viewport dimensions are too large");
        assert!(surface_dimensions.1 <= ctxt.capabilities.max_viewport_dims.1 as u32,
                "Viewport dimensions are too large");

        let viewport = (0, 0, surface_dimensions.0 as gl::types::GLsizei,
                        surface_dimensions.1 as gl::types::GLsizei);

        if ctxt.state.viewport != Some(viewport) {
            unsafe { ctxt.gl.Viewport(viewport.0, viewport.1, viewport.2, viewport.3); }
            ctxt.state.viewport = Some(viewport);
        }
    }

    // scissor
    if let Some(scissor) = scissor {
        let scissor = (scissor.left as gl::types::GLint, scissor.bottom as gl::types::GLint,
                       scissor.width as gl::types::GLsizei,
                       scissor.height as gl::types::GLsizei);

        unsafe {
            if ctxt.state.scissor != Some(scissor) {
                ctxt.gl.Scissor(scissor.0, scissor.1, scissor.2, scissor.3);
                ctxt.state.scissor = Some(scissor);
            }

            if !ctxt.state.enabled_scissor_test {
                ctxt.gl.Enable(gl::SCISSOR_TEST);
                ctxt.state.enabled_scissor_test = true;
            }
        }
    } else {
        unsafe {
            if ctxt.state.enabled_scissor_test {
                ctxt.gl.Disable(gl::SCISSOR_TEST);
                ctxt.state.enabled_scissor_test = false;
            }
        }
    }
}

fn sync_rasterizer_discard(ctxt: &mut context::CommandContext, draw_primitives: bool) {
    if ctxt.state.enabled_rasterizer_discard == draw_primitives {
        if ctxt.version >= &Version(Api::Gl, 3, 0) {
            if draw_primitives {
                unsafe { ctxt.gl.Disable(gl::RASTERIZER_DISCARD); }
                ctxt.state.enabled_rasterizer_discard = false;
            } else {
                unsafe { ctxt.gl.Enable(gl::RASTERIZER_DISCARD); }
                ctxt.state.enabled_rasterizer_discard = true;
            }

        } else if ctxt.extensions.gl_ext_transform_feedback {
            if draw_primitives {
                unsafe { ctxt.gl.Disable(gl::RASTERIZER_DISCARD_EXT); }
                ctxt.state.enabled_rasterizer_discard = false;
            } else {
                unsafe { ctxt.gl.Enable(gl::RASTERIZER_DISCARD_EXT); }
                ctxt.state.enabled_rasterizer_discard = true;
            }

        } else {
            unreachable!();
        }
    }
}

unsafe fn sync_vertices_per_patch(ctxt: &mut context::CommandContext, vertices_per_patch: Option<u16>) {
    if let Some(vertices_per_patch) = vertices_per_patch {
        let vertices_per_patch = vertices_per_patch as gl::types::GLint;
        if ctxt.state.patch_patch_vertices != vertices_per_patch {
            ctxt.gl.PatchParameteri(gl::PATCH_VERTICES, vertices_per_patch);
            ctxt.state.patch_patch_vertices = vertices_per_patch;
        }
    }
}
