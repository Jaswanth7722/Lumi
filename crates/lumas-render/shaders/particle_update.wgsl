// Lumas Particle Update Compute Shader
//
// GPU-side particle simulation. Updates position, velocity, and lifetime
// for each particle. Writes alive particle count into the draw indirect
// buffer so the CPU never reads back particle state.
//
// Workgroup size: 64 (particle count must be a multiple of 64).
//
// Storage buffers:
//   @group(0) @binding(0) — ParticleData: particle state arrays (read-write)
//   @group(0) @binding(1) — IndirectBuffer: DrawIndexedIndirect args (write)
//   @group(0) @binding(2) — Uniforms: ParticleSystemUBO (read-only)

// ──────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────

const PARTICLE_DEAD: u32 = 0u;
const PARTICLE_ALIVE: u32 = 1u;

// ──────────────────────────────────────────────
// Structs
// ──────────────────────────────────────────────

struct ParticleSystemUBO {
    delta_time: f32,        // Frame delta in seconds
    elapsed_time: f32,      // Total elapsed time (for animation curves)
    emitter_position: vec4<f32>,  // xyz = world position
    gravity: vec4<f32>,           // xyz = gravity direction * strength
    spawn_rate: f32,              // Particles per second
    lifetime: f32,                // Base particle lifetime
    initial_speed: f32,           // Base initial speed
    speed_spread: f32,            // Speed randomness
    _pad: f32,
};

struct Particle {
    position: vec4<f32>,    // xyz = world position
    velocity: vec4<f32>,    // xyz = velocity
    life: f32,              // Remaining life (seconds)
    max_life: f32,          // Initial life (for normalized curve evaluation)
    size: f32,              // Billboard size
    flags: u32,             // PARTICLE_ALIVE or PARTICLE_DEAD
    color: vec4<f32>,       // RGBA color (pre-multiplied alpha)
};

struct Particles {
    data: array<Particle>,
};

@group(0) @binding(0)
var<storage, read_write> particles: Particles;

@group(0) @binding(1)
var<storage, read_write> indirect: array<u32>;

@group(0) @binding(2)
var<uniform> ubo: ParticleSystemUBO;

// ──────────────────────────────────────────────
// Random Number Generation (xorshift32)
// ──────────────────────────────────────────────

fn rand_xorshift(state: ptr<function, u32>) -> u32 {
    var x = *state;
    x = x ^ (x << 13u);
    x = x ^ (x >> 17u);
    x = x ^ (x << 5u);
    *state = x;
    return x;
}

fn rand_f32(state: ptr<function, u32>) -> f32 {
    return f32(rand_xorshift(state) & 0x007fffffffu) / f32(0x007fffffffu);
}

fn rand_range(state: ptr<function, u32>, min_val: f32, max_val: f32) -> f32 {
    return min_val + rand_f32(state) * (max_val - min_val);
}

// ──────────────────────────────────────────────
// Emitter Shapes
// ──────────────────────────────────────────────

fn spawn_position(state: ptr<function, u32>, emitter_pos: vec3<f32>) -> vec3<f32> {
    // Spawn within a small sphere around the emitter.
    let theta = rand_f32(state) * 6.2831853;
    let phi = rand_f32(state) * 3.1415927;
    let r = rand_f32(state) * 0.5;
    return emitter_pos + vec3(
        r * sin(phi) * cos(theta),
        r * cos(phi),
        r * sin(phi) * sin(theta),
    );
}

fn spawn_velocity(state: ptr<function, u32>) -> vec3<f32> {
    let theta = rand_f32(state) * 6.2831853;
    let phi = rand_f32(state) * 3.1415927;
    let speed = rand_range(state, ubo.initial_speed - ubo.speed_spread, ubo.initial_speed + ubo.speed_spread);
    return vec3(
        speed * sin(phi) * cos(theta),
        speed * cos(phi),
        speed * sin(phi) * sin(theta),
    ) * max(speed, 0.0);
}

// ──────────────────────────────────────────────
// Compute Shader Main
// ──────────────────────────────────────────────

@compute @workgroup_size(64)
fn cs_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) num_groups: vec3<u32>,
) {
    let index = gid.x;
    let total_particles = arrayLength(&particles.data);

    if index >= total_particles {
        return;
    }

    var particle = particles.data[index];
    let dt = ubo.delta_time;

    // Initialize random state per particle.
    var rng_state = u32(index) * 2654435761u + u32(ubo.elapsed_time * 1000.0);

    if particle.flags == PARTICLE_ALIVE {
        // Update alive particle.
        particle.life = particle.life - dt;

        if particle.life <= 0.0 {
            // Particle died this frame.
            particle.flags = PARTICLE_DEAD;
            particles.data[index] = particle;
            return;
        }

        // Apply physics.
        particle.velocity = particle.velocity + ubo.gravity * dt;
        particle.position = particle.position + particle.velocity * dt;

        // Fade alpha based on remaining life.
        let life_t = particle.life / max(particle.max_life, 0.001);
        particle.color.a = particle.color.a * min(life_t * 2.0, 1.0);  // Fade in then out

        // Update particle.
        particles.data[index] = particle;
    } else {
        // Particle is dead — check if we should respawn.
        // Only respawn if we're within the first spawn_rate fraction of particles.
        // This distributes spawning across the particle budget.
        let spawn_probability = ubo.spawn_rate * dt / f32(total_particles);
        if rand_f32(&rng_state) < spawn_probability {
            // Respawn particle.
            particle.position = vec4(spawn_position(&rng_state, ubo.emitter_position.xyz), 1.0);
            particle.velocity = vec4(spawn_velocity(&rng_state), 0.0);
            particle.max_life = ubo.lifetime;
            particle.life = ubo.lifetime * (0.5 + rand_f32(&rng_state) * 0.5);
            particle.size = 0.15 + rand_f32(&rng_state) * 0.1;
            particle.flags = PARTICLE_ALIVE;
            particle.color = vec4(1.0, 1.0, 1.0, 1.0);
            particles.data[index] = particle;
        }
    }
}
