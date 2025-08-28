//! Data Handling Engine — modular & self-contained
//!
//! Modules: mapping, ga_filter, cleaning, integrity.
//! See Cargo.toml deps at bottom of this comment.

pub mod mapping {
    use strsim::levenshtein;
    use std::collections::{HashMap, HashSet};
    use xxhash_rust::xxh3::xxh3_64;

    #[derive(Debug)]
    pub struct HybridSimilaritySearch {
        lsh: Lsh,
        k: usize,
        num_hashes: usize,
        signatures: HashMap<String, Vec<u64>>,
    }
    impl HybridSimilaritySearch {
        pub fn new(k: usize, num_hashes: usize, bands: usize, rows: usize) -> Self {
            Self { lsh: Lsh::new(bands, rows), k, num_hashes, signatures: HashMap::new() }
        }
        pub fn insert(&mut self, key: &str, text: &str) {
            let sig = minhash_signature(text, self.k, self.num_hashes);
            self.lsh.insert(key, &sig);
            self.signatures.insert(key.to_string(), sig);
        }
        pub fn insert_batch(&mut self, items: &[(&str, &str)]) { for (k,t) in items { self.insert(k,t);} }
        pub fn best_match(&self, query: &str) -> Option<(String, usize)> {
            let qsig = minhash_signature(query, self.k, self.num_hashes);
            let cands = self.lsh.candidates(&qsig);
            let mut best: Option<(String, usize)> = None;
            for key in cands {
                let d = levenshtein(&query.to_ascii_uppercase(), &key.to_ascii_uppercase());
                if best.as_ref().map(|(_,bd)| d<*bd).unwrap_or(true) { best = Some((key.clone(), d)); }
            }
            best
        }
        pub fn matches_within_distance(&self, query:&str, m:usize)->Vec<(String,usize)>{
            let qsig = minhash_signature(query, self.k, self.num_hashes);
            let cands = self.lsh.candidates(&qsig);
            let mut out=Vec::new();
            for key in cands {
                let d = levenshtein(&query.to_ascii_uppercase(), &key.to_ascii_uppercase());
                if d<=m { out.push((key,d)); }
            }
            out.sort_by(|a,b| a.1.cmp(&b.1)); out
        }
        pub fn top_matches(&self, query:&str, n:usize)->Vec<(String,usize)>{
            let qsig = minhash_signature(query, self.k, self.num_hashes);
            let cands = self.lsh.candidates(&qsig);
            let mut v:Vec<(String,usize)> = cands.into_iter().map(|k| {
                let d = levenshtein(&query.to_ascii_uppercase(), &k.to_ascii_uppercase()); (k,d)
            }).collect();
            v.sort_by(|a,b| a.1.cmp(&b.1)); v.into_iter().take(n).collect()
        }
        pub fn similarity_score(&self, a:&str, b:&str)->f64{
            let d = levenshtein(a,b) as f64; let m = a.len().max(b.len()) as f64;
            if m==0.0 {1.0} else {1.0 - d/m}
        }
        pub fn jaccard_estimate(&self, a:&str, b:&str)->f64{
            if let (Some(sa),Some(sb)) = (self.signatures.get(a), self.signatures.get(b)) {
                let eq = sa.iter().zip(sb.iter()).filter(|(x,y)| x==y).count();
                eq as f64 / self.num_hashes as f64
            } else { 0.0 }
        }
    }
    fn shingles(s:&str, k:usize)->impl Iterator<Item=u64> + '_ {
        s.to_ascii_uppercase().as_bytes().windows(k).map(|w| xxh3_64(w))
    }
    pub fn minhash_signature(s:&str, k:usize, num_hashes:usize)->Vec<u64>{
        let sh:Vec<u64> = shingles(s,k).collect();
        if sh.is_empty(){ return vec![u64::MAX; num_hashes]; }
        (0..num_hashes).map(|i|{
            sh.iter()
              .map(|h| xxh3_64(&((h ^ (i as u64).wrapping_mul(0x9E3779B185EBCA87))).to_le_bytes()))
              .min().unwrap()
        }).collect()
    }
    #[derive(Debug)]
    pub struct Lsh{ bands:usize, rows:usize, tables:Vec<HashMap<Vec<u64>, HashSet<String>>> }
    impl Lsh{
        pub fn new(bands:usize, rows:usize)->Self{
            let mut t=Vec::with_capacity(bands); for _ in 0..bands { t.push(HashMap::new()); } Self{bands,rows,tables:t}
        }
        pub fn insert(&mut self, key:&str, sig:&[u64]){
            for b in 0..self.bands {
                let start=b*self.rows; let end=start+self.rows; if end>sig.len(){continue;}
                let slice = sig[start..end].to_vec(); self.tables[b].entry(slice).or_default().insert(key.to_string());
            }
        }
        pub fn candidates(&self, sig:&[u64])->HashSet<String>{
            let mut out=HashSet::new();
            for b in 0..self.bands{
                let start=b*self.rows; let end=start+self.rows; if end>sig.len(){continue;}
                let slice=sig[start..end].to_vec();
                if let Some(s)=self.tables[b].get(&slice){ out.extend(s.iter().cloned()); }
            } out
        }
    }
}

pub mod ga_filter {
    use rand::Rng;
    use std::cmp::Ordering;

    #[derive(Clone, Copy, Debug)]
    pub struct GAParams{ pub population_size:usize, pub max_generations:usize, pub elite_size:usize, pub crossover_rate:f64, pub inject_every:usize, pub bounds:(f64,f64) }
    impl Default for GAParams{ fn default()->Self{ Self{ population_size:20, max_generations:150, elite_size:4, crossover_rate:0.6, inject_every:20, bounds:(-10.0,10.0) } } }

    #[derive(Clone, Copy, Debug)]
    pub struct Kalman1D{ pub q:f64, pub r:f64, pub x:f64, pub p:f64 }
    impl Kalman1D{ pub fn new(q:f64,r:f64,init:f64)->Self{Self{q,r,x:init,p:1.0}} pub fn update(&mut self,z:f64)->f64{ self.p+=self.q; let k=self.p/(self.p+self.r); self.x+=k*(z-self.x); self.p*=1.0-k; self.x } }

    #[derive(Debug)]
    pub struct HybridGAFilter{ pub ga_params:GAParams, pub kalman_q:f64, pub kalman_r:f64, pub outlier_threshold:f64 }
    impl HybridGAFilter{
        pub fn new(ga:GAParams,q:f64,r:f64,thr:f64)->Self{ Self{ga_params:ga, kalman_q:q, kalman_r:r, outlier_threshold:thr} }
        pub fn optimize_kalman_params<F>(&mut self, data:&[f64], fitness:F)->(f64,f64) where F: Fn(f64,f64,&[f64])->f64 + Copy {
            let fit = |q:f64,r:f64|{ let mut k=Kalman1D::new(q,r,data[0]); let filt:Vec<f64>=data.iter().map(|&z|k.update(z)).collect(); 0.7*fitness(q,r,&filt)+0.3*fitness(q,r,data) };
            let best_q = run(self.ga_params, |q| fit(q,self.kalman_r));
            let best_r = run(self.ga_params, |r| fit(self.kalman_q,r));
            self.kalman_q=best_q; self.kalman_r=best_r; (best_q,best_r)
        }
        fn remove_outliers(&self, orig:&[f64], filt:&[f64])->Vec<f64>{ orig.iter().zip(filt.iter()).map(|(&o,&f)| if (o-f).abs()<self.outlier_threshold {o}else{f}).collect() }
        pub fn forecast(&mut self, data:&[f64], horizon:usize)->Vec<f64>{
            let _ = self.optimize_kalman_params(data, |q,r,d|{ let mut k=Kalman1D::new(q,r,d[0]); let mut err=0.0; for i in 1..d.len(){ let p=k.update(d[i]); if i<d.len()-1{ err += (d[i+1]-p).powi(2); } } -err/(d.len().saturating_sub(2) as f64) });
            let mut k=Kalman1D::new(self.kalman_q,self.kalman_r,data[0]); let last=data.iter().copied().fold(data[0], |_,z| k.update(z)); vec![last; horizon]
        }
        pub fn detect_anomalies(&mut self, data:&[f64])->Vec<bool>{
            let _ = self.optimize_kalman_params(data, |q,r,d|{ let mut k=Kalman1D::new(q,r,d[0]); let mut res=Vec::with_capacity(d.len()); for &z in d{ let p=k.update(z); res.push((z-p).abs()); } let mut s=res.clone(); s.sort_by(|a,b| a.partial_cmp(b).unwrap()); let idx=((s.len() as f64)*0.95) as usize; let thr=*s.get(idx.min(s.len().saturating_sub(1))).unwrap_or(&0.0); let an:Vec<f64>=res.into_iter().filter(|&r| r>thr).collect(); if an.is_empty(){0.0}else{ let mean=an.iter().sum::<f64>()/an.len() as f64; let ratio=(an.len() as f64)/(s.len().max(1) as f64); if ratio>0.3{0.0}else{mean*(1.0-ratio)}} });
            let mut k=Kalman1D::new(self.kalman_q,self.kalman_r,data[0]); data.iter().map(|&z|{ let p=k.update(z); (z-p).abs()>self.outlier_threshold }).collect()
        }
    }
    pub fn mutate(x:f64,g:usize,maxg:usize,rng:&mut impl Rng,lo:f64,hi:f64)->f64{ let prog=g as f64/maxg as f64; let base=(1.0-0.8*prog).max(0.05); let mut y= if rng.gen_bool(0.3){x + rng.gen_range(-1..=1) as f64 * base}else{x + rng.gen_range(-base..base)}; if y<lo{y=lo+(lo-y).abs().min(1.0);} if y>hi{y=hi-(y-hi).abs().min(1.0);} y }
    pub fn crossover(a:f64,b:f64,rng:&mut impl Rng)->f64{ let alpha=rng.gen_range(0.0..1.0); alpha*a + (1.0-alpha)*b }
    pub fn run<F>(p:GAParams, mut f:F)->f64 where F: FnMut(f64)->f64 {
        let mut rng=rand::thread_rng(); let (lo,hi)=p.bounds; let mut pop:(Vec<f64>)=(0..p.population_size).map(|_| rng.gen_range(lo..hi)).collect();
        let mut best=pop[0]; let mut bestf=f(best);
        for g in 0..p.max_generations {
            pop.sort_by(|a,b| f(*b).partial_cmp(&f(*a)).unwrap_or(Ordering::Equal));
            let top=f(pop[0]); if top>bestf{bestf=top; best=pop[0];}
            if top.is_finite() && (bestf-top).abs()<1e-9 && g>p.max_generations/4 { break; }
            let mut np=pop[..p.elite_size].to_vec();
            while np.len()<p.population_size {
                if rng.gen::<f64>()<p.crossover_rate { let p1=pop[rng.gen_range(0..p.elite_size)]; let p2=pop[rng.gen_range(0..p.elite_size)]; np.push(mutate(crossover(p1,p2,&mut rng),g,p.max_generations,&mut rng,lo,hi)); }
                else { let p0=pop[rng.gen_range(0..p.elite_size)]; np.push(mutate(p0,g,p.max_generations,&mut rng,lo,hi)); }
            }
            if p.inject_every>0 && g%p.inject_every==0 { let n=np.len(); let c=p.population_size/10; for i in (n-c)..n { np[i]=rng.gen_range(lo..hi);} }
            pop=np;
        } *pop.iter().max_by(|a,b| f(**a).partial_cmp(&f(**b)).unwrap()).unwrap()
    }
}

pub mod cleaning {
    use serde::{Deserialize, Serialize};
    use super::ga_filter::Kalman1D;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IfConfig{ pub window:usize, pub contamination:f32, pub score_threshold:f32 }

    pub trait OutlierDetector{ fn score(&self, window:&[(f64,f64)])->f32; fn is_anomaly(&self, window:&[(f64,f64)])->bool{ self.score(window) >= 0.5 } }

    pub struct HybridKalmanMad{ kalman:Kalman1D, residuals:Vec<f64>, window_size:usize, pub score_threshold:f32 }
    impl HybridKalmanMad{
        pub fn new(cfg:&IfConfig, q:f64, r:f64)->Self{ Self{ kalman:Kalman1D::new(q,r,0.0), residuals:Vec::with_capacity(cfg.window), window_size:cfg.window, score_threshold:cfg.score_threshold } }
    }
    impl OutlierDetector for HybridKalmanMad{
        fn score(&self, window:&[(f64,f64)])->f32{
            if window.len()<2 { return 0.0; }
            let mut k=self.kalman; let mut res=self.residuals.clone();
            for &(_,v) in window.iter().take(window.len()-1){ let p=k.update(v); res.push(v-p); if res.len()>self.window_size { res.remove(0); } }
            let &(_, last)=window.last().unwrap(); let p=k.update(last); let cur=last-p;
            if res.len()<5 { return 0.0; }
            let mut s=res.clone(); s.sort_by(|a,b| a.total_cmp(b)); let med=s[s.len()/2];
            let mut devs:Vec<f64>=s.iter().map(|x|(x-med).abs()).collect(); devs.sort_by(|a,b| a.total_cmp(b)); let mad=devs[devs.len()/2].max(1e-9);
            let score=(cur.abs()/(1.4826*mad)).min(10.0)/10.0; score as f32
        }
    }
}

pub mod integrity {
    use sha2::{Sha256, Digest};
    use rs_merkle::{algorithms::Sha256 as Algo, MerkleTree};
    use std::collections::HashMap;

    #[derive(Default, Debug)]
    pub struct HybridHasher{ leaves:Vec<[u8;32]>, data_map:HashMap<String, Vec<usize>> }
    impl HybridHasher{
        pub fn new()->Self{ Self{leaves:Vec::new(), data_map:HashMap::new()} }
        pub fn add_data(&mut self, data:&str)->([u8;32],usize){
            let h=sha256_bytes(data.as_bytes()); let idx=self.leaves.len(); self.leaves.push(h); self.data_map.entry(data.to_string()).or_default().push(idx); (h,idx)
        }
        pub fn add_batch(&mut self, items:&[&str])->Vec<([u8;32],usize)>{ items.iter().map(|&d| self.add_data(d)).collect() }
        pub fn root_hex(&self)->String{ if self.leaves.is_empty(){return String::new();} let tree=MerkleTree::<Algo>::from_leaves(&self.leaves); match tree.root(){Some(h)=>hex::encode(h),None=>String::new()} }
        pub fn generate_proof(&self, data:&str)->Option<(rs_merkle::MerkleProof<Algo>,usize)>{ let idx=*self.data_map.get(data)?.first()?; let tree=MerkleTree::<Algo>::from_leaves(&self.leaves); Some((tree.proof(&[idx]), idx)) }
        pub fn verify_data(&self, data:&str)->bool{
            if let Some((proof, idx))=self.generate_proof(data){ let tree=MerkleTree::<Algo>::from_leaves(&self.leaves); if let Some(root)=tree.root(){ let leaf=sha256_bytes(data.as_bytes()); return proof.verify(root, &[idx], &[leaf], self.leaves.len()); } } false
        }
        pub fn generate_batch_proof(&self, items:&[&str])->Option<(rs_merkle::MerkleProof<Algo>, Vec<usize>)>{
            let mut idxs=Vec::new(); for d in items { idxs.push(*self.data_map.get(*d)?.first()?); } let tree=MerkleTree::<Algo>::from_leaves(&self.leaves); Some((tree.proof(&idxs), idxs))
        }
        pub fn verify_batch(&self, items:&[&str])->bool{
            if let Some((proof,idxs))=self.generate_batch_proof(items){ let tree=MerkleTree::<Algo>::from_leaves(&self.leaves); if let Some(root)=tree.root(){ let leaves:Vec<[u8;32]>=items.iter().map(|&d| sha256_bytes(d.as_bytes())).collect(); return proof.verify(root,&idxs,&leaves,self.leaves.len()); } } false
        }
        pub fn find_collisions(&self)->HashMap<[u8;32], Vec<String>>{
            let mut m:HashMap<[u8;32],Vec<String>>=HashMap::new();
            for (data, idxs) in &self.data_map { if let Some(&i)=idxs.first(){ let h=self.leaves[i]; m.entry(h).or_default().push(data.clone()); } }
            m.retain(|_,v| v.len()>1); m
        }
        pub fn export_tree(&self)->(Vec<[u8;32]>, String){ (self.leaves.clone(), self.root_hex()) }
        pub fn import_tree(&mut self, leaves:Vec<[u8;32]>, items:Option<Vec<String>>){ self.leaves=leaves; self.data_map.clear(); if let Some(xs)=items{ for (i,s) in xs.into_iter().enumerate(){ self.data_map.entry(s).or_default().push(i);} } }
    }
    pub fn sha256_hex(s:&str)->String{ hex::encode(sha256_bytes(s.as_bytes())) }
    pub fn sha256_bytes(b:&[u8])->[u8;32]{ let mut h=Sha256::new(); h.update(b); let out=h.finalize(); let mut a=[0u8;32]; a.copy_from_slice(&out); a }
    pub fn root_hex_from_leaves(leaves:&[[u8;32]])->String{ if leaves.is_empty(){return String::new();} let tree=MerkleTree::<Algo>::from_leaves(leaves); match tree.root(){Some(h)=>hex::encode(h),None=>String::new()} }
}

pub struct DataHandlingEngine{
    pub mapper: mapping::HybridSimilaritySearch,
    pub cleaner: cleaning::HybridKalmanMad,
    pub ga: ga_filter::HybridGAFilter,
    pub hasher: integrity::HybridHasher,
}
impl DataHandlingEngine{
    pub fn new_default()->Self{
        let mapper=mapping::HybridSimilaritySearch::new(3,60,12,5);
        let cleaner=cleaning::HybridKalmanMad::new(&cleaning::IfConfig{window:64, contamination:0.01, score_threshold:0.7}, 0.001, 0.05);
        let ga=ga_filter::HybridGAFilter::new(ga_filter::GAParams::default(), 0.001, 0.05, 1.0);
        let hasher=integrity::HybridHasher::new();
        Self{ mapper, cleaner, ga, hasher }
    }
}

// Cargo.toml deps needed:
// serde = { version = "1.0", features = ["derive"] }
// rand = "0.8"
// strsim = "0.10"
// xxhash-rust = { version = "0.8", features = ["xxh3"] }
// sha2 = "0.10"
// rs_merkle = "1.5"
// hex = "0.4"
