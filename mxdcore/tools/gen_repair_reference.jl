# MEDDLY reference for the repair/restart boundary numbers.
#
# Same non-monotone system as boundary_nonmonotone.jl -- distribution_system(n),
# Sedlacek et al. 2021 §4.2 -- but over four transition relations rather than one,
# so the repair and restart cases the manuscript wants are covered:
#
#   dec      x_i -> x_i - 1        one component degrades one step
#   inc      x_i -> x_i + 1        one component is repaired one step
#   restart  x_i -> top            one component is replaced/restarted outright
#   loop     dec | restart         the repair/restart loop: degrade gradually,
#                                  restore to new
#
# For each level j both crossing directions are recorded:
#   up   = R ∩ (L_j × U_j)   entering {φ >= j}
#   down = R ∩ (U_j × L_j)   leaving it
#
# MEDDLY's global state is only freed at cleanup(), and its forests segfault if
# Julia's GC runs over them, so everything measured is held in KEEP.

using MDDMinsol, Meddly, Printf

const T, YMIN, YMAX = 3, 5, 20
const KEEP = Any[]

function main(path, ns)
    Meddly.initialize()
    open(path, "w") do io
        println(io, "n,relation,j,up_card,down_card,up_nodes,down_nodes,rel_card,rel_nodes")
        for n in ns
            sys = distribution_system(n; t=T, ymin=YMIN, ymax=YMAX); push!(KEEP, sys)
            relf = relation_forest(sys); push!(KEEP, relf)

            dec     = decrement_relation(relf, sys)
            inc     = increment_relation(relf, sys)
            restart = repair_relation(relf, sys)
            loop    = dec | restart
            for r in (dec, inc, restart, loop); push!(KEEP, r); end

            for (name, R) in ["dec"=>dec, "inc"=>inc, "restart"=>restart, "loop"=>loop]
                for j in 1:(sys.m - 1)
                    up   = boundary_level(relf, sys, j, R);      push!(KEEP, up)
                    down = boundary_level_down(relf, sys, j, R); push!(KEEP, down)
                    @printf(io, "%d,%s,%d,%s,%s,%d,%d,%s,%d\n",
                            n, name, j,
                            string(BigInt(round(cardinality(up)))),
                            string(BigInt(round(cardinality(down)))),
                            mdd_node_count(up), mdd_node_count(down),
                            string(BigInt(round(cardinality(R)))),
                            mdd_node_count(R))
                end
            end
            flush(io)
            println("n=$n done")
        end
    end
    println("wrote ", path)
end

main(ARGS[1], [parse(Int, a) for a in ARGS[2:end]])
