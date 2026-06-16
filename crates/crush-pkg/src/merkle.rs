use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// MerkleNode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleNode {
    pub hash: [u8; 32],
    pub left: Option<Box<MerkleNode>>,
    pub right: Option<Box<MerkleNode>>,
    pub data: Option<Vec<u8>>,
    pub level: u32,
}

impl MerkleNode {
    pub fn leaf(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update([0x00]);
        hasher.update(data);
        let hash: [u8; 32] = hasher.finalize().into();
        Self {
            hash,
            left: None,
            right: None,
            data: Some(data.to_vec()),
            level: 0,
        }
    }

    pub fn internal(left: MerkleNode, right: MerkleNode) -> Self {
        let lh: [u8; 32] = left.hash;
        let rh: [u8; 32] = right.hash;
        let mut hasher = Sha256::new();
        hasher.update([0x01]);
        hasher.update(&lh);
        hasher.update(&rh);
        let level = left.level.max(right.level) + 1;
        Self {
            hash: hasher.finalize().into(),
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            data: None,
            level,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.data.is_some()
    }

    pub fn is_internal(&self) -> bool {
        self.data.is_none()
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }

    pub fn verify_hash(&self) -> anyhow::Result<()> {
        let expected: [u8; 32] = match &self.data {
            Some(data) => {
                let mut h = Sha256::new();
                h.update([0x00]);
                h.update(data);
                h.finalize().into()
            }
            None => {
                let (l, r) = match (&self.left, &self.right) {
                    (Some(l), Some(r)) => (l.as_ref(), r.as_ref()),
                    _ => anyhow::bail!("internal node without children"),
                };
                let mut h = Sha256::new();
                h.update([0x01]);
                h.update(&l.hash);
                h.update(&r.hash);
                h.finalize().into()
            }
        };
        if expected == self.hash {
            Ok(())
        } else {
            anyhow::bail!("node hash mismatch: expected {}, got {}",
                hex::encode(expected), hex::encode(self.hash))
        }
    }

    pub fn depth(&self) -> u32 {
        if self.is_leaf() {
            0
        } else {
            let ld = self.left.as_ref().map(|n| n.depth()).unwrap_or(0);
            let rd = self.right.as_ref().map(|n| n.depth()).unwrap_or(0);
            1 + ld.max(rd)
        }
    }

    pub fn root_hash(&self) -> [u8; 32] {
        self.hash
    }
}

impl Default for MerkleNode {
    fn default() -> Self {
        Self {
            hash: [0; 32],
            left: None,
            right: None,
            data: None,
            level: 0,
        }
    }
}

impl PartialEq for MerkleNode {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for MerkleNode {}

// ---------------------------------------------------------------------------
// MerkleTree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleTree {
    pub root: Option<MerkleNode>,
    pub leaf_count: usize,
    pub height: u32,
}

impl MerkleTree {
    pub fn from_data(data: Vec<Vec<u8>>) -> anyhow::Result<Self> {
        if data.is_empty() {
            return Ok(Self { root: None, leaf_count: 0, height: 0 });
        }
        let leaves: Vec<MerkleNode> = data.into_iter()
            .map(|d| MerkleNode::leaf(&d))
            .collect();
        Self::build_tree(leaves)
    }

    fn build_tree(nodes: Vec<MerkleNode>) -> anyhow::Result<Self> {
        let leaf_count = nodes.len();
        if leaf_count == 1 {
            let root = nodes.into_iter().next().unwrap();
            let height = root.depth();
            return Ok(Self { root: Some(root), leaf_count, height });
        }

        let mut cur = nodes;
        while cur.len() > 1 {
            let mut next = Vec::new();
            for i in (0..cur.len()).step_by(2) {
                if i + 1 < cur.len() {
                    next.push(MerkleNode::internal(cur[i].clone(), cur[i + 1].clone()));
                } else {
                    next.push(cur[i].clone());
                }
            }
            cur = next;
        }

        let root = cur.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("empty tree after build"))?;
        let height = root.depth();
        Ok(Self { root: Some(root), leaf_count, height })
    }

    pub fn empty() -> Self {
        Self { root: None, leaf_count: 0, height: 0 }
    }

    pub fn root_hash(&self) -> Option<[u8; 32]> {
        self.root.as_ref().map(|r| r.root_hash())
    }

    pub fn root_hash_hex(&self) -> Option<String> {
        self.root_hash().map(|h| hex::encode(h))
    }

    pub fn verify(&self) -> anyhow::Result<()> {
        if let Some(ref root) = self.root {
            verify_node_hash(root)?;
        }
        Ok(())
    }

    pub fn verify_inclusion(&self, data: &[u8]) -> anyhow::Result<bool> {
        let leaf_hash = hash_leaf(data);
        self.verify_hash_inclusion(&leaf_hash)
    }

    pub fn verify_hash_inclusion(&self, target: &[u8; 32]) -> anyhow::Result<bool> {
        match &self.root {
            Some(root) => search_hash(root, target),
            None => Ok(false),
        }
    }

    pub fn leaf_hashes(&self) -> Vec<[u8; 32]> {
        self.root.as_ref().map_or_else(Vec::new, |r| collect_leaf_hashes(r))
    }

    pub fn stats(&self) -> MerkleTreeStats {
        MerkleTreeStats {
            leaf_count: self.leaf_count,
            height: self.height,
            total_nodes: self.root.as_ref().map_or(0, |r| count_nodes(r)),
            root_hash: self.root_hash(),
        }
    }
}

fn search_hash(node: &MerkleNode, target: &[u8; 32]) -> anyhow::Result<bool> {
    if node.hash == *target {
        return Ok(node.is_leaf());
    }
    if let (Some(l), Some(r)) = (&node.left, &node.right) {
        if search_hash(l, target)? { return Ok(true); }
        if search_hash(r, target)? { return Ok(true); }
    }
    Ok(false)
}

fn collect_leaf_hashes(node: &MerkleNode) -> Vec<[u8; 32]> {
    if node.is_leaf() {
        vec![node.hash]
    } else {
        let mut h = Vec::new();
        if let Some(l) = &node.left { h.extend(collect_leaf_hashes(l)); }
        if let Some(r) = &node.right { h.extend(collect_leaf_hashes(r)); }
        h
    }
}

fn count_nodes(node: &MerkleNode) -> usize {
    if node.is_leaf() { 1 }
    else {
        let l = node.left.as_ref().map(|n| count_nodes(n)).unwrap_or(0);
        let r = node.right.as_ref().map(|n| count_nodes(n)).unwrap_or(0);
        1 + l + r
    }
}

fn verify_node_hash(node: &MerkleNode) -> anyhow::Result<()> {
    node.verify_hash()?;
    if let (Some(l), Some(r)) = (&node.left, &node.right) {
        verify_node_hash(l)?;
        verify_node_hash(r)?;
    } else if node.is_internal() {
        anyhow::bail!("internal node without children");
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleTreeStats {
    pub leaf_count: usize,
    pub height: u32,
    pub total_nodes: usize,
    pub root_hash: Option<[u8; 32]>,
}

// ---------------------------------------------------------------------------
// Hashing utilities
// ---------------------------------------------------------------------------

pub fn hash_leaf(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x00]);
    h.update(data);
    let r: [u8; 32] = h.finalize().into();
    r
}

pub fn hash_internal(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x01]);
    h.update(left);
    h.update(right);
    let r: [u8; 32] = h.finalize().into();
    r
}

pub fn hash_with_domain(domain: &str, data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(domain.as_bytes());
    h.update([0x00]);
    h.update(data);
    let r: [u8; 32] = h.finalize().into();
    r
}

pub fn compute_merkle_root(hashes: &[[u8; 32]]) -> anyhow::Result<[u8; 32]> {
    if hashes.is_empty() {
        anyhow::bail!("empty hash list");
    }
    let mut cur: Vec<[u8; 32]> = hashes.to_vec();
    while cur.len() > 1 {
        let mut next = Vec::new();
        for i in (0..cur.len()).step_by(2) {
            if i + 1 < cur.len() {
                next.push(hash_internal(&cur[i], &cur[i + 1]));
            } else {
                next.push(cur[i]);
            }
        }
        cur = next;
    }
    Ok(cur[0])
}

pub fn verify_batch_hashes(hashes: &[[u8; 32]], expected: &[u8; 32]) -> anyhow::Result<bool> {
    let computed = compute_merkle_root(hashes)?;
    Ok(computed == *expected)
}

// ---------------------------------------------------------------------------
// MerkleProof
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleProof {
    pub leaf_hash: [u8; 32],
    pub proof_hashes: Vec<[u8; 32]>,
    pub leaf_index: u32,
    pub total_leaves: u32,
}

impl MerkleProof {
    pub fn new(leaf_hash: [u8; 32], proof_hashes: Vec<[u8; 32]>, leaf_index: u32, total_leaves: u32) -> Self {
        Self { leaf_hash, proof_hashes, leaf_index, total_leaves }
    }

    pub fn proof_size(&self) -> usize {
        self.proof_hashes.len()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.leaf_index >= self.total_leaves {
            anyhow::bail!("leaf index {} out of range (total: {})", self.leaf_index, self.total_leaves);
        }
        if self.proof_hashes.len() > 32 {
            anyhow::bail!("proof too long");
        }
        Ok(())
    }

    pub fn verify(&self, root_hash: &[u8; 32]) -> anyhow::Result<bool> {
        self.validate()?;
        let mut computed = self.leaf_hash;
        for (i, sibling) in self.proof_hashes.iter().enumerate() {
            let is_left = (self.leaf_index >> i) & 1 == 0;
            computed = if is_left {
                hash_internal(&computed, sibling)
            } else {
                hash_internal(sibling, &computed)
            };
        }
        Ok(&computed == root_hash)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(44 + self.proof_hashes.len() * 32);
        buf.extend_from_slice(&self.leaf_hash);
        buf.extend_from_slice(&self.leaf_index.to_le_bytes());
        buf.extend_from_slice(&self.total_leaves.to_le_bytes());
        buf.extend_from_slice(&(self.proof_hashes.len() as u32).to_le_bytes());
        for h in &self.proof_hashes {
            buf.extend_from_slice(h);
        }
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 44 {
            anyhow::bail!("proof bytes too short");
        }
        let mut off = 0;
        let leaf_hash: [u8; 32] = bytes[off..off+32].try_into()?; off += 32;

        let read_u32 = |b: &[u8], o: &mut usize| -> anyhow::Result<u32> {
            let arr: [u8; 4] = b[*o..*o+4].try_into()?;
            *o += 4;
            Ok(u32::from_le_bytes(arr))
        };
        let leaf_index = read_u32(bytes, &mut off)?;
        let total_leaves = read_u32(bytes, &mut off)?;
        let count = read_u32(bytes, &mut off)?;

        let mut proof_hashes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let arr: [u8; 32] = bytes[off..off+32].try_into()?;
            off += 32;
            proof_hashes.push(arr);
        }
        Ok(Self { leaf_hash, proof_hashes, leaf_index, total_leaves })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_tree() {
        let tree = MerkleTree::from_data(vec![
            b"data1".to_vec(),
            b"data2".to_vec(),
        ]).unwrap();
        assert!(tree.root_hash().is_some());
        assert_eq!(tree.leaf_count, 2);
        assert!(tree.verify().is_ok());
    }

    #[test]
    fn empty_tree() {
        let tree = MerkleTree::from_data(vec![]).unwrap();
        assert!(tree.root_hash().is_none());
        assert_eq!(tree.leaf_count, 0);
    }

    #[test]
    fn single_leaf() {
        let tree = MerkleTree::from_data(vec![b"only".to_vec()]).unwrap();
        assert_eq!(tree.leaf_count, 1);
        assert!(tree.verify().is_ok());
    }

    #[test]
    fn odd_leaves() {
        let tree = MerkleTree::from_data(vec![
            b"a".to_vec(), b"b".to_vec(), b"c".to_vec(),
        ]).unwrap();
        assert_eq!(tree.leaf_count, 3);
        assert!(tree.verify().is_ok());
    }

    #[test]
    fn verify_inclusion() {
        let data = vec![
            b"alpha".to_vec(),
            b"beta".to_vec(),
            b"gamma".to_vec(),
        ];
        let tree = MerkleTree::from_data(data.clone()).unwrap();
        assert!(tree.verify_inclusion(&data[0]).unwrap());
        assert!(tree.verify_inclusion(&data[1]).unwrap());
        assert!(tree.verify_inclusion(&data[2]).unwrap());
        assert!(!tree.verify_inclusion(b"delta").unwrap());
    }

    #[test]
    fn proof_verify() {
        // Tree: [a, b, c, d]
        // root = hash_internal(hash_internal(a,b), hash_internal(c,d))
        let a = hash_leaf(b"a");
        let b = hash_leaf(b"b");
        let c = hash_leaf(b"c");
        let d = hash_leaf(b"d");
        let root = hash_internal(&hash_internal(&a, &b), &hash_internal(&c, &d));

        // Proof for b (index 1): sibling = a, then sibling = hash_internal(c,d)
        let proof = MerkleProof::new(b, vec![a, hash_internal(&c, &d)], 1, 4);
        assert!(proof.verify(&root).unwrap());
    }

    #[test]
    fn proof_bytes_roundtrip() {
        let proof = MerkleProof::new(
            [1u8; 32],       // leaf hash
            vec![[2u8; 32]],  // one sibling
            0,                // index
            2,                // total
        );
        let bytes = proof.to_bytes();
        let restored = MerkleProof::from_bytes(&bytes).unwrap();
        assert_eq!(restored.proof_size(), 1);
        assert_eq!(restored.leaf_index, 0);
    }

    #[test]
    fn compute_root_matches_tree() {
        let data: Vec<Vec<u8>> = vec![
            b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec(),
        ];
        let tree = MerkleTree::from_data(data.clone()).unwrap();
        let hashes: Vec<[u8; 32]> = data.iter().map(|d| hash_leaf(d)).collect();
        let computed = compute_merkle_root(&hashes).unwrap();
        assert_eq!(Some(computed), tree.root_hash());
    }

    #[test]
    fn leaf_hashes_count() {
        let tree = MerkleTree::from_data(vec![
            b"1".to_vec(), b"2".to_vec(), b"3".to_vec(), b"4".to_vec(), b"5".to_vec(),
        ]).unwrap();
        assert_eq!(tree.leaf_hashes().len(), 5);
    }

    #[test]
    fn domain_separation() {
        let h1 = hash_with_domain("app", b"data");
        let h2 = hash_with_domain("other", b"data");
        assert_ne!(h1, h2);
    }
}
