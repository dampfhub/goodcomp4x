#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec4 surface;
layout(location = 2) in vec2 worldPosition;
layout(location = 0) out vec4 outColor;

const vec3 SUN = vec3(-0.48, 0.36, 0.80);

float hash(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x),
               mix(hash(i + vec2(0, 1)), hash(i + vec2(1)), f.x), f.y);
}
float fbm(vec2 p) {
    float result = 0.0, weight = 0.5;
    for (int i = 0; i < 5; ++i) {
        result += weight * noise(p);
        p = mat2(1.6, 1.2, -1.2, 1.6) * p + 7.3;
        weight *= 0.5;
    }
    return result;
}
vec3 finishColor(vec3 color) {
    // Soft photographic shoulder, in linear space (the swapchain is sRGB).
    return color / (vec3(1.0) + color) * 1.35;
}
float elevation(vec2 p, vec2 origin, float kind) {
    vec2 w = p + origin;
    float base = 0.006 * fbm(w * 13.0);
    if (kind < 0.5 || kind > 2.5) return base;
    vec2 warped = p + 0.10 * vec2(noise(w * 9.0) - 0.5, noise(w * 9.0 + 31.0) - 0.5);
    float envelope = pow(max(0.0, 1.0 - length(warped * vec2(1.05, 0.92)) / 0.94), 1.3);
    float ridges = 1.0 - abs(2.0 * fbm(w * 3.0) - 1.0);
    if (kind < 1.5) return base + envelope * 0.24 * (0.6 + 0.4 * ridges);
    // Intersecting, asymmetric ridgelines instead of a round mound per hex.
    float peakA = max(0.0, 1.0 - length((warped - vec2(-0.21, 0.10)) * vec2(1.8, 1.05)));
    float peakB = max(0.0, 1.0 - length((warped - vec2(0.19, -0.28)) * vec2(1.35, 1.9)));
    float peakC = max(0.0, 1.0 - length((warped - vec2(0.23, 0.38)) * vec2(2.4, 2.1)));
    float peaks = max(peakA, max(peakB * 0.87, peakC * 0.72));
    return base + peaks * smoothstep(0.0, 0.18, envelope)
        * (0.83 + ridges * 0.35 + 0.045 * fbm(w * 24.0));
}
vec3 terrain() {
    vec2 p = surface.xy;
    vec2 origin = worldPosition - p;
    float kind = surface.w;
    float h = elevation(p, origin, kind);
    const float e = 0.006;
    vec2 slope = vec2(elevation(p + vec2(e, 0), origin, kind)
                   - elevation(p - vec2(e, 0), origin, kind),
                     elevation(p + vec2(0, e), origin, kind)
                   - elevation(p - vec2(0, e), origin, kind)) / (2.0 * e);
    vec3 normal = normalize(vec3(-slope, 1.0));
    float broad = fbm(worldPosition * 2.4);
    float detail = noise(worldPosition * 95.0);
    // Derivative filtering keeps fine soil and grass stable when zoomed out.
    float detailWeight = 1.0 - smoothstep(0.012, 0.055, length(fwidth(worldPosition)));
    vec3 grass = mix(vec3(0.028, 0.052, 0.012), vec3(0.16, 0.19, 0.050), broad);
    vec3 earth = mix(vec3(0.095, 0.058, 0.030), vec3(0.25, 0.20, 0.125), broad);
    float path = 1.0 - smoothstep(0.05, 0.17,
        abs(worldPosition.x + 0.23 * sin(worldPosition.y * 1.7)));
    vec3 albedo = mix(grass, earth, max(path * 0.7, smoothstep(0.54, 0.76, broad)));
    float rock = kind > 1.5 && kind < 2.5 ? smoothstep(0.04, 0.27, h) : 0.0;
    vec3 stone = mix(vec3(0.085, 0.095, 0.10), vec3(0.34, 0.32, 0.28), fbm(worldPosition * 12.0));
    albedo = mix(albedo, stone, rock);
    float snow = smoothstep(0.64, 1.0, h + 0.08 * noise(worldPosition * 24.0))
               * smoothstep(0.4, 0.85, normal.z);
    albedo = mix(albedo, vec3(0.81, 0.86, 0.88), snow);
    albedo *= 0.86 + 0.28 * mix(0.5, detail, detailWeight);
    float shadow = 1.0;
    if (kind > 0.5 && kind < 2.5) {
        for (int i = 1; i <= 9; ++i) {
            float t = float(i) * 0.075;
            float blocker = elevation(p + SUN.xy * t, origin, kind) - h - SUN.z * t;
            shadow = min(shadow, 1.0 - smoothstep(-0.025, 0.06, blocker) * 0.75);
        }
    }
    float diffuse = max(dot(normal, SUN), 0.0);
    vec3 color = albedo * (vec3(0.34, 0.41, 0.47) + vec3(1.7, 1.47, 1.13) * diffuse * shadow);
    if (kind > 2.5) color *= 0.5;
    return finishColor(color);
}

float boxSdf(vec3 p, vec3 size) {
    vec3 q = abs(p) - size;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}
float ellipsoid(vec3 p, vec3 r) {
    float k0 = length(p / r), k1 = length(p / (r * r));
    return k0 * (k0 - 1.0) / max(k1, 0.0001);
}
float capsule(vec3 p, vec3 a, vec3 b, float r) {
    vec3 v = b - a;
    return length(p - a - v * clamp(dot(p - a, v) / dot(v, v), 0.0, 1.0)) - r;
}
vec2 unite(vec2 a, vec2 b) { return a.x < b.x ? a : b; }
// Material IDs: aged steel, team cloth, leather/wood, brass.
vec2 soldier(vec3 p, float kind) {
    vec2 d = vec2(ellipsoid(p - vec3(0, 0, 0.64), vec3(0.18, 0.12, 0.25)), 1);
    d = unite(d, vec2(ellipsoid(p - vec3(0, 0, 0.95), vec3(0.13, 0.12, 0.15)), 0));
    d = unite(d, vec2(boxSdf(p - vec3(0, -0.111, 0.94), vec3(0.085, 0.014, 0.019)), 2));
    for (int i = -1; i <= 1; i += 2) {
        float s = float(i);
        d = unite(d, vec2(capsule(p, vec3(s * 0.09, 0, 0.48), vec3(s * 0.12, -0.04, 0.15), 0.067), 0));
        d = unite(d, vec2(ellipsoid(p - vec3(s * 0.12, -0.08, 0.12), vec3(0.075, 0.13, 0.07)), 2));
        d = unite(d, vec2(capsule(p, vec3(s * 0.16, 0, 0.77), vec3(s * 0.24, -0.09, 0.54), 0.067), 0));
    }
    if (kind < 0.5) {
        d = unite(d, vec2(ellipsoid(p - vec3(-0.25, -0.15, 0.55), vec3(0.20, 0.055, 0.26)), 1));
        d = unite(d, vec2(ellipsoid(p - vec3(-0.25, -0.20, 0.55), vec3(0.055, 0.028, 0.055)), 3));
        d = unite(d, vec2(boxSdf(p - vec3(0.25, -0.10, 0.85), vec3(0.027, 0.018, 0.29)), 0));
        d = unite(d, vec2(boxSdf(p - vec3(0.25, -0.10, 0.59), vec3(0.09, 0.026, 0.018)), 3));
    } else {
        vec3 q = p - vec3(0.29, -0.08, 0.65);
        float bow = max(abs(length(q.xz) - 0.32) - 0.022, abs(q.y) - 0.025);
        bow = max(bow, -q.x);
        d = unite(d, vec2(bow, 2));
        d = unite(d, vec2(capsule(p, vec3(0.29, -0.08, 0.33), vec3(0.29, -0.08, 0.97), 0.007), 3));
    }
    return d;
}
vec2 miniature(vec3 p) {
    float kind = surface.w;
    if (kind > 3.5) {
        vec2 tree = vec2(capsule(p, vec3(0, 0, 0.03), vec3(0, 0, 0.95), 0.036), 2);
        for (int i = 0; i < 4; ++i) {
            float layer = float(i);
            float z = p.z - (0.32 + layer * 0.19);
            float cone = max(length(p.xy) - (0.31 - layer * 0.055) * (1.0 - z / 0.40), max(-z, z - 0.40));
            tree = unite(tree, vec2(cone * 0.65, 4));
        }
        return tree;
    }
    // Low, weathered metal plinth anchors every model to the battlefield.
    vec2 d = vec2(max(length(p.xy) - 0.67, abs(p.z - 0.045) - 0.045), 3);
    if (kind < 1.5) return unite(d, soldier(p, kind));
    if (kind < 2.5) {
        d = unite(d, vec2(ellipsoid(p - vec3(0, 0, 0.47), vec3(0.21, 0.38, 0.22)), 2));
        d = unite(d, vec2(capsule(p, vec3(0, -0.22, 0.5), vec3(0, -0.36, 0.86), 0.125), 2));
        d = unite(d, vec2(ellipsoid(p - vec3(0, -0.46, 0.84), vec3(0.11, 0.20, 0.12)), 2));
        for (int i = 0; i < 4; ++i) {
            vec2 leg = vec2(i < 2 ? -0.14 : 0.14, (i % 2 == 0) ? -0.23 : 0.23);
            d = unite(d, vec2(capsule(p, vec3(leg, 0.42), vec3(leg + vec2(0, -0.045), 0.12), 0.047), 2));
        }
        d = unite(d, soldier((p - vec3(0, 0.02, 0.45)) / 0.67, 0.0) * vec2(0.67, 1));
        d = unite(d, vec2(capsule(p, vec3(0.22, 0, 0.45), vec3(0.22, -0.08, 1.3), 0.018), 3));
        return d;
    }
    // Siege engine: timber chassis, four iron-bound wheels, raised throwing arm.
    d = unite(d, vec2(boxSdf(p - vec3(0, 0, 0.27), vec3(0.30, 0.40, 0.07)), 2));
    for (int i = 0; i < 4; ++i) {
        vec3 q = p - vec3(i < 2 ? -0.36 : 0.36, i % 2 == 0 ? -0.28 : 0.28, 0.23);
        d = unite(d, vec2(max(length(q.yz) - 0.19, abs(q.x) - 0.05), 0));
        d = unite(d, vec2(max(length(q.yz) - 0.145, abs(q.x) - 0.057), 2));
    }
    d = unite(d, vec2(capsule(p, vec3(-0.23, 0.15, 0.3), vec3(0, 0, 0.8), 0.045), 2));
    d = unite(d, vec2(capsule(p, vec3(0.23, 0.15, 0.3), vec3(0, 0, 0.8), 0.045), 2));
    d = unite(d, vec2(capsule(p, vec3(0, -0.33, 0.38), vec3(0, 0.28, 1.04), 0.052), 2));
    d = unite(d, vec2(ellipsoid(p - vec3(0, 0.28, 1.04), vec3(0.15, 0.14, 0.065)), 0));
    return d;
}
vec4 renderMiniature() {
    vec2 uv = surface.xy;
    float filtered = 1.0 - smoothstep(0.015, 0.06, length(fwidth(uv)));
    vec3 ray = vec3(0, 0.625, -0.780625);
    vec3 origin = vec3(uv.x, uv.y * 0.780625 - 2.5, uv.y * 0.625 + 3.52);
    float t = 2.0;
    vec2 hit = vec2(1.0);
    vec3 p;
    for (int i = 0; i < 72; ++i) {
        p = origin + ray * t;
        hit = miniature(p);
        if (hit.x < 0.002 || t > 5.4) break;
        t += max(hit.x * 0.75, 0.001);
    }
    if (hit.x > 0.008 || t > 5.4) {
        float shadow = exp(-5.0 * dot(uv - vec2(0.14, -0.19), uv - vec2(0.14, -0.19)));
        return vec4(0.012, 0.015, 0.009, shadow * 0.42 * fragColor.a);
    }
    const vec2 e = vec2(0.002, 0);
    vec3 n = normalize(vec3(miniature(p + e.xyy).x - miniature(p - e.xyy).x,
                            miniature(p + e.yxy).x - miniature(p - e.yxy).x,
                            miniature(p + e.yyx).x - miniature(p - e.yyx).x));
    float m = hit.y;
    vec3 albedo = vec3(0.31, 0.36, 0.39);
    float metal = 0.85, roughness = 0.28;
    if (m > 0.5 && m < 1.5) { albedo = fragColor.rgb * 0.65; metal = 0.05; roughness = 0.72; }
    if (m > 1.5 && m < 2.5) { albedo = vec3(0.17, 0.073, 0.029); metal = 0.0; roughness = 0.8; }
    if (m > 2.5) { albedo = vec3(0.32, 0.23, 0.10); metal = 0.75; roughness = 0.36; }
    if (m > 3.5) { albedo = vec3(0.028, 0.075, 0.024); metal = 0.0; roughness = 1.0; }
    float grain = noise(p.xy * 180.0 + p.z * 83.0);
    albedo *= mix(1.0, 0.80 + grain * 0.30, filtered);
    float shadow = 1.0;
    for (int i = 1; i <= 12; ++i) {
        float distanceToLight = float(i) * 0.045;
        float d = miniature(p + n * 0.012 + SUN * distanceToLight).x;
        shadow = min(shadow, clamp(12.0 * d / distanceToLight, 0.15, 1.0));
    }
    float ao = 1.0;
    for (int i = 1; i <= 3; ++i) {
        float r = float(i) * 0.05;
        ao -= max(0.0, r - miniature(p + n * r).x) * 1.9;
    }
    vec3 halfVector = normalize(SUN - ray);
    float spec = pow(max(dot(n, halfVector), 0.0), mix(100.0, 12.0, roughness));
    vec3 fresnel = mix(vec3(0.04), albedo, metal);
    vec3 color = albedo * vec3(0.38, 0.44, 0.52) * clamp(ao, 0.25, 1.0)
               + (albedo * max(dot(n, SUN), 0.0) * (1.0 - metal * 0.4)
               + fresnel * spec * 3.5) * vec3(1.8, 1.55, 1.2) * shadow;
    return vec4(finishColor(color), fragColor.a);
}
void main() {
    if (surface.z < 0.5) outColor = fragColor;
    else if (surface.z < 1.5) outColor = vec4(terrain(), fragColor.a);
    else outColor = renderMiniature();
}
