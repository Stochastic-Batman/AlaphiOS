use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Lazy;
use crate::sync::spinlock::Spinlock;

pub type DomainId = u32;
pub type ObjectId = u64;
pub type RoleId = u32;


#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rights(pub u32);

impl Rights {
    pub const NONE: Self = Self(0);
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const EXECUTE: u32 = 1 << 2;
    pub const COPY: u32 = 1 << 3;
    pub const OWNER: u32 = 1 << 4;
    pub const CONTROL: u32 = 1 << 5;

    pub fn contains(self, bits: u32) -> bool {
        self.0 & bits == bits
    }
}


pub trait AccessControl {
    fn access(&self, domain: DomainId, object: ObjectId) -> Rights;
    fn grant(&mut self, grantor: DomainId, target: DomainId, object: ObjectId, rights: Rights) -> Result<(), ()>;
    fn revoke(&mut self, revoker: DomainId, target: DomainId, object: ObjectId, rights: Rights) -> Result<(), ()>;
}


struct AccessEntry {
    domain: DomainId,
    rights: Rights,
}

struct AccessList {
    objects: BTreeMap<ObjectId, Vec<AccessEntry>>,
}

impl AccessList {
    fn new() -> Self {
        Self { objects: BTreeMap::new() }
    }

    fn lookup(&self, domain: DomainId, object: ObjectId) -> Rights {
        self.objects.get(&object)
            .and_then(|entries| entries.iter().find(|e| e.domain == domain))
            .map(|e| e.rights)
            .unwrap_or(Rights::NONE)
    }

    fn set(&mut self, domain: DomainId, object: ObjectId, rights: Rights) {
        let entries = self.objects.entry(object).or_insert_with(Vec::new);
        if rights.0 == 0 {
            entries.retain(|e| e.domain != domain);
        } else if let Some(entry) = entries.iter_mut().find(|e| e.domain == domain) {
            entry.rights = rights;
        } else {
            entries.push(AccessEntry { domain, rights });
        }
    }
}


pub struct RbacAccessControl {
    acl: AccessList,
    roles: BTreeMap<RoleId, Rights>,
    user_roles: BTreeMap<DomainId, RoleId>,
}

impl RbacAccessControl {
    pub fn new() -> Self {
        Self {
            acl: AccessList::new(),
            roles: BTreeMap::new(),
            user_roles: BTreeMap::new(),
        }
    }

    pub fn define_role(&mut self, role: RoleId, rights: Rights) {
        self.roles.insert(role, rights);
    }

    pub fn assign_role(&mut self, domain: DomainId, role: RoleId) {
        self.user_roles.insert(domain, role);
    }

    pub fn set_rights(&mut self, domain: DomainId, object: ObjectId, rights: Rights) {
        self.acl.set(domain, object, rights);
    }

    fn role_rights(&self, domain: DomainId) -> Rights {
        self.user_roles.get(&domain)
            .and_then(|role| self.roles.get(role))
            .copied()
            .unwrap_or(Rights::NONE)
    }
}

impl AccessControl for RbacAccessControl {
    fn access(&self, domain: DomainId, object: ObjectId) -> Rights {
        let acl = self.acl.lookup(domain, object);
        let role = self.role_rights(domain);
        Rights(acl.0 | role.0)
    }

    fn grant(&mut self, grantor: DomainId, target: DomainId, object: ObjectId, rights: Rights) -> Result<(), ()> {
        let g = self.access(grantor, object);

        if g.contains(Rights::OWNER) || g.contains(Rights::CONTROL) {
            let current = self.acl.lookup(target, object);
            self.acl.set(target, object, Rights(current.0 | rights.0));
            return Ok(());
        }

        if g.contains(Rights::COPY) {
            let copyable = g.0 & rights.0;
            if copyable == rights.0 {
                let current = self.acl.lookup(target, object);
                self.acl.set(target, object, Rights(current.0 | rights.0));
                return Ok(());
            }
        }

        Err(())
    }

    fn revoke(&mut self, revoker: DomainId, target: DomainId, object: ObjectId, rights: Rights) -> Result<(), ()> {
        let r = self.access(revoker, object);

        if r.contains(Rights::OWNER) || r.contains(Rights::CONTROL) {
            let current = self.acl.lookup(target, object);
            self.acl.set(target, object, Rights(current.0 & !rights.0));
            return Ok(());
        }

        Err(())
    }
}


pub static ACCESS_CONTROL: Lazy<Spinlock<RbacAccessControl>> = Lazy::new(|| {
    Spinlock::new(RbacAccessControl::new())
});
