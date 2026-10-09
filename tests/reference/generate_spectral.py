"""Independent scientific references, stdlib only. Regenerate from repository root.
MassBank CC BY input records are unmodified; source/attribution hashes in spectral_sources.json.
Unique-mass cosine uses dense all-pair dot products without greedy implementation.
Formula reference exhaustively enumerates every CHNO atom count using Decimal.
"""
from pathlib import Path
from decimal import Decimal
from itertools import product
import json, math
root=Path(__file__).parent

def peaks(name):
    text=(root/name).read_text()
    block=text.split('PK$PEAK: m/z int. rel.int.\n')[1].split('//')[0]
    return [[float(x) for x in line.split()[:2]] for line in block.splitlines() if line.strip()]
a=peaks('MSBNK-Antwerp_Univ-AN111301.txt')
b=peaks('MSBNK-Antwerp_Univ-AN111302.txt')
comparisons=[]
for threshold in [0.0,0.01,0.1]:
    aa=[p for p in a if p[1]>=max(q[1] for q in a)*threshold]
    bb=[p for p in b if p[1]>=max(q[1] for q in b)*threshold]
    pairs=[(i,j) for i,p in enumerate(aa) for j,q in enumerate(bb) if abs(p[0]-q[0])<=0.02]
    assert len({i for i,j in pairs})==len(pairs)==len({j for i,j in pairs}), 'Reference assumes unambiguous masses'
    cosine=sum(aa[i][1]*bb[j][1] for i,j in pairs)/math.sqrt(sum(p[1]**2 for p in aa)*sum(p[1]**2 for p in bb))
    comparisons.append(dict(relative_threshold=threshold,cosine=cosine,matches=len(pairs)))
masses=[Decimal(x) for x in ['12','1.00782503223','14.00307400443','15.99491461957']]
observed=Decimal('195.087652032')
formula=[]
for counts in product(range(13),range(31),range(7),range(7)):
    c,h,n,o=counts
    dbe=Decimal(1+c)+Decimal(n-h)/2
    if dbe<0 or dbe%1:continue
    mass=sum(v*x for v,x in zip(masses,counts))
    for adduct,offset in [('[M+H]+',Decimal('1.007276466621')),('[M+Na]+',Decimal('22.989220702091'))]:
        predicted=mass+offset
        if abs(predicted-observed)<=observed*Decimal('0.000005'):
            formula.append({'formula':''.join(sym+(str(x) if x>1 else '') for sym,x in zip(['C','H','N','O'],counts) if x),'adduct':adduct,'neutral_mass_da':float(mass),'predicted_mz':float(predicted),'dbe':float(dbe)})
(root/'spectral.json').write_text(json.dumps({'generator':'Python stdlib Decimal exhaustive CHNO and unique-mass dot product','massbank_cross_energy':comparisons,'formulas':formula,'carbon10_m1_relative_to_m0':float(Decimal(10)*Decimal('0.0107')/Decimal('0.9893'))},indent=2)+'\n')
