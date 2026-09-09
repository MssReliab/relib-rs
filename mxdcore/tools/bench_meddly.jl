# MEDDLY side of the boundary-operator timing comparison.
#
# Self-contained: it drives Meddly.jl directly rather than going through
# MDDMinsol, because MDDMinsol's `distribution_system` is hardcoded to three
# states per component (`fill(3, n)`, no `states` argument) and the comparison
# needs to vary that axis. Nothing is written to the research repository.
#
# The system is the same one: n factories, production P = t·Σxᵢ, and
#     φ = 0 if P < ymin,  1 if P > ymax,  2 otherwise
# so producing more can drop φ from 2 to 1. Level sets are built by thresholding
# the integer sum, which is what MDDMinsol does too.
#
# Timed phases, matching the Rust harness:
#   t_rel  building the transition relation
#   t_bnd  every level's boundary, both directions, and its cardinality
#
# φ construction is NOT timed: MEDDLY builds it as an MTMDD and thresholds,
# the Rust side builds the level sets directly, and those are different work.
#
# MEDDLY's forests segfault if Julia's GC runs over them, so everything measured
# is held in KEEP.
#
# Usage:
#   MEDDLY_JL=~/path/to/Meddly.jl
#   cd $MEDDLY_JL && LIBMEDDLY_C_PATH=$MEDDLY_JL/c/libmeddly_c.dylib \
#       julia --project=. bench_meddly.jl <out.csv> <n> <states> [<n> <states> ...]
#
# One process per (n, states) is the recommended protocol -- MEDDLY's global
# state is only freed at cleanup(), so later conditions in a shared process run
# slower. Passing several pairs runs them together, which amortises warm-up
# instead; the harness driving this runs both and keeps whichever is faster.

using Meddly
using Printf

const T, YMIN, YMAX = 3, 5, 20
const KEEP = Any[]
const TIMEOUT_S = 60.0

"φ as a function of the component sum."
phi(s) = (p = T * s; p < YMIN ? 0 : p > YMAX ? 1 : 2)

"Level sets of φ over `n` components of `states` states each."
function level_sets(n::Int, states::Int)
    b = mdd()
    for i in 1:n
        defvar!(b, Symbol("x", i), i, collect(0:(states - 1)))
    end
    vars = [var!(b, Symbol("x", i)) for i in 1:n]
    intf = vars[1].forest
    rf = MDDForestBoolMxD(intf.domain)
    push!(KEEP, b); push!(KEEP, rf)
    for v in vars; push!(KEEP, v); end

    # Σ xᵢ as an integer MDD, then threshold it. φ = 2 on the band
    # [ceil(ymin/t), floor(ymax/t)], 1 above it, 0 below.
    total = reduce(+, vars)
    push!(KEEP, total)
    lo = cld(YMIN, T)
    hi = fld(YMAX, T)
    u1 = total >= Edge(intf, lo)                                # φ ≥ 1
    u2 = and(total >= Edge(intf, lo), total <= Edge(intf, hi))  # φ ≥ 2
    uppers = [u1, u2]
    lowers = [lnot(u1), lnot(u2)]
    for e in vcat(uppers, lowers); push!(KEEP, e); end
    return rf, uppers, lowers
end

"One component steps `from → to`, every other component invariant."
transition(rf, n, i, from, to) =
    mxd_singleton(rf, [k == i ? from : -1 for k in 1:n],
                      [k == i ? to   : -2 for k in 1:n])

function relation(rf, n::Int, states::Int, kind::Symbol)
    steps(v) = kind === :dec     ? [(v, v-1) for v in 1:(states-1)] :
               kind === :inc     ? [(v, v+1) for v in 0:(states-2)] :
               kind === :restart ? [(v, states-1) for v in 0:(states-2)] :
               error("unknown relation $kind")
    rel = nothing
    for i in 1:n, (from, to) in steps(0)
        e = transition(rf, n, i, from, to)
        rel = rel === nothing ? e : or(rel, e)
    end
    rel
end

function measure(io, n::Int, states::Int)
    rf, uppers, lowers = level_sets(n, states)

    t_rel = @elapsed begin
        dec = relation(rf, n, states, :dec)
        inc = relation(rf, n, states, :inc)
        restart = relation(rf, n, states, :restart)
        families = ["dec_inc" => or(dec, inc),
                    "restart" => restart,
                    "dec_restart" => or(dec, restart)]
    end
    for (_, r) in families; push!(KEEP, r); end

    for (name, R) in families
        cards = Int[]
        t_bnd = @elapsed begin
            for j in 1:2
                up   = and(cross(lowers[j], uppers[j], rf), R)
                down = and(cross(uppers[j], lowers[j], rf), R)
                push!(KEEP, up); push!(KEEP, down)
                push!(cards, Int(round(cardinality(up))))
                push!(cards, Int(round(cardinality(down))))
            end
        end
        @printf(io, "%d,%d,%s,%.6g,%.6g,%s\n",
                n, states, name, t_rel, t_bnd, join(cards, ";"))
    end
    flush(io)
end

function main(path, pairs)
    Meddly.initialize()
    open(path, "w") do io
        println(io, "n,states,relation,t_rel_s,t_bnd_s,cards")
        # Warm up so the first measured condition is not paying for JIT.
        let (rf, u, l) = level_sets(3, 3)
            r = relation(rf, 3, 3, :dec)
            push!(KEEP, r)
            _ = and(cross(l[1], u[1], rf), r)
        end
        for (n, states) in pairs
            t0 = time()
            measure(io, n, states)
            el = time() - t0
            @printf(stderr, "n=%d states=%d done in %.2fs\n", n, states, el)
            if el > TIMEOUT_S
                @printf(stderr, "  (over %.0fs -- stopping this run)\n", TIMEOUT_S)
                break
            end
        end
    end
    println("wrote ", path)
end

let a = ARGS
    path = a[1]
    nums = parse.(Int, a[2:end])
    @assert iseven(length(nums)) "give (n, states) pairs"
    main(path, [(nums[i], nums[i+1]) for i in 1:2:length(nums)])
end
