struct BlurParams_std140_0
{
    @align(16) width_0 : u32,
    @align(4) height_0 : u32,
    @align(8) radius_0 : u32,
    @align(4) _pad_0 : u32,
};

@group(0) @binding(0) var<uniform> params_0 : BlurParams_std140_0;
@group(0) @binding(1) var<storage, read> src_0 : array<u32>;

@group(0) @binding(2) var<storage, read_write> dst_0 : array<u32>;

fn rgba8_unpack_0( p_0 : u32) -> vec4<f32>
{
    return vec4<f32>(f32((p_0 & (u32(255)))), f32((((p_0 >> (u32(8)))) & (u32(255)))), f32((((p_0 >> (u32(16)))) & (u32(255)))), f32((((p_0 >> (u32(24)))) & (u32(255))))) / vec4<f32>(255.0f);
}

fn rgba8_pack_0( v_0 : vec4<f32>) -> u32
{
    var _S1 : vec4<u32> = vec4<u32>(saturate(v_0) * vec4<f32>(255.0f) + vec4<f32>(0.5f));
    return ((((((_S1.x) | ((((_S1.y) << (u32(8))))))) | ((((_S1.z) << (u32(16))))))) | ((((_S1.w) << (u32(24))))));
}

struct Neighborhood_0
{
     padded_width_0 : u32,
     padded_height_0 : u32,
     center_offset_0 : vec2<u32>,
     center_size_0 : vec2<u32>,
     radius_1 : i32,
};

fn Neighborhood_bind_0( _S2 : u32,  _S3 : u32,  _S4 : i32) -> Neighborhood_0
{
    var n_0 : Neighborhood_0;
    n_0.padded_width_0 = _S2;
    n_0.padded_height_0 = _S3;
    var _S5 : u32 = u32(_S4);
    n_0.center_offset_0 = vec2<u32>(_S5, _S5);
    var _S6 : u32 = u32(2) * _S5;
    n_0.center_size_0 = vec2<u32>(_S2 - _S6, _S3 - _S6);
    n_0.radius_1 = _S4;
    return n_0;
}

fn Neighborhood_load_0( _S7 : Neighborhood_0,  _S8 : vec2<i32>) -> vec4<f32>
{
    var clamped_xy_0 : vec2<i32> = clamp(_S8, vec2<i32>(i32(0), i32(0)), vec2<i32>(i32(_S7.padded_width_0) - i32(1), i32(_S7.padded_height_0) - i32(1)));
    return rgba8_unpack_0(src_0[u32(clamped_xy_0.y) * _S7.padded_width_0 + u32(clamped_xy_0.x)]);
}

fn stencil_sum_0( _S9 : Neighborhood_0,  _S10 : vec2<u32>,  _S11 : i32,  _S12 : ptr<function, u32>) -> vec4<f32>
{
    (*_S12) = u32(0);
    const _S13 : vec4<f32> = vec4<f32>(0.0f, 0.0f, 0.0f, 0.0f);
    var _S14 : i32 = - _S11;
    var dy_0 : i32 = _S14;
    var sum_0 : vec4<f32> = _S13;
    for(;;)
    {
        if(dy_0 <= _S11)
        {
        }
        else
        {
            break;
        }
        var dx_0 : i32 = _S14;
        for(;;)
        {
            if(dx_0 <= _S11)
            {
            }
            else
            {
                break;
            }
            var sum_1 : vec4<f32> = sum_0 + Neighborhood_load_0(_S9, vec2<i32>(_S10) + vec2<i32>(dx_0, dy_0));
            (*_S12) = (*_S12) + u32(1);
            dx_0 = dx_0 + i32(1);
            sum_0 = sum_1;
        }
        dy_0 = dy_0 + i32(1);
    }
    return sum_0;
}

fn box_blur_0( _S15 : Neighborhood_0,  _S16 : vec2<u32>,  _S17 : u32) -> vec4<f32>
{
    var count_0 : u32;
    var _S18 : vec4<f32> = stencil_sum_0(_S15, _S16, i32(_S17), &(count_0));
    return _S18 / vec4<f32>(f32(count_0));
}

@compute
@workgroup_size(8, 8, 1)
fn cs_blur(@builtin(global_invocation_id) gid_0 : vec3<u32>)
{
    var _S19 : u32 = gid_0.x;
    var _S20 : bool;
    if(_S19 >= (params_0.width_0))
    {
        _S20 = true;
    }
    else
    {
        _S20 = (gid_0.y) >= (params_0.height_0);
    }
    if(_S20)
    {
        return;
    }
    var _S21 : Neighborhood_0 = Neighborhood_bind_0(params_0.width_0, params_0.height_0, i32(params_0.radius_0));
    dst_0[gid_0.y * params_0.width_0 + _S19] = rgba8_pack_0(box_blur_0(_S21, _S21.center_offset_0 + gid_0.xy, params_0.radius_0));
    return;
}

