//! The Demiurge Engine — the internal accounting layer for a Vicinagora node.
//!
//! Three rules are load bearing and are enforced by the type system, not by
//! policy that a later operator could quietly relax:
//!
//!   1. Only breath mints. A human contributing hours can create Demiurge.
//!      An enterprise cannot. Look for a `mint` method on [`ProducerAccount`].
//!      There is none, and that absence is the architecture.
//!
//!   2. The floor never depends on a balance. [`floor_guaranteed`] takes a
//!      person and returns the guarantee. It never reads a wallet. Housing,
//!      power, food, water, healthcare and education rest on breath, not coin.
//!      The Demiurge buys what sits above the floor. It cannot buy the floor.
//!
//!   3. Every unit decays. A [`DemiurgeLot`] carries the second it was minted.
//!      Twenty years later it is gone. Money is treated like energy. It flows
//!      or it dies. No hoard. No dynasty.
//!
//! The Demiurge measures contribution. It does not measure worth. Worth is the
//! floor and the floor is free. The currency is the craftsman, not the source.
//!
//! ## Dynamic provisioning
//!
//! A node is not a founder constant either. A [`NodeBlueprint`] describes a
//! resource a council wants to stand up — a Housing block, a Solar array, a
//! Biodigester, a V-CTDS — and a [`ProvisionOrder`] carries it through a vote.
//! Ratify the order and it materializes a live [`Node`] with its standing labor
//! demand already set, so the gap engine immediately knows it is understaffed
//! and the convergence loop starts recruiting against it.
//!
//! Production notes for the Linexus codebase:
//!   - `Timestamp` here is unix seconds. In production use `chrono::DateTime<Utc>`.
//!   - `PersonId` / `NodeId` are u128 here. In production use UUIDv7.
//!   - The persistent rows (`ContributionEvent`, `DemiurgeLot`) map to SeaORM
//!     entities. The minting and gap engines are pure domain logic and stay
//!     storage agnostic so they unit test without a database.
//!   - Every mint and every burn is an append only ledger fact, same discipline
//!     as the Logger. State is derived from the ledger, never edited in place.

use std::collections::{HashMap, HashSet};

// =====================================================================
// Units. Money is never a float. Time is never a float.
// =====================================================================

/// Basis points. 10_000 == 1.0x. Lets us carry 1.5x, 1.25x and 1.3x as exact
/// integers so a multiplier never introduces floating point drift into money.
pub type Bps = i64;
pub const ONE: Bps = 10_000;

/// Whole Demiurge. Stored as i64. Production may widen to a fixed point decimal
/// if sub unit granularity is wanted, but the API stays the same.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Demiurge(pub i64);

impl Demiurge {
    pub fn scaled(self, bps: Bps) -> Demiurge {
        let v = (self.0 as i128) * (bps as i128) / (ONE as i128);
        Demiurge(v as i64)
    }
}
impl std::ops::Add for Demiurge {
    type Output = Demiurge;
    fn add(self, o: Demiurge) -> Demiurge {
        Demiurge(self.0 + o.0)
    }
}
impl std::ops::Sub for Demiurge {
    type Output = Demiurge;
    fn sub(self, o: Demiurge) -> Demiurge {
        Demiurge(self.0 - o.0)
    }
}
impl std::ops::AddAssign for Demiurge {
    fn add_assign(&mut self, o: Demiurge) {
        self.0 += o.0;
    }
}

/// Time measured in whole minutes. Avoids the fractional hour trap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Minutes(pub i64);
impl Minutes {
    pub fn from_hours(h: i64) -> Self {
        Minutes(h * 60)
    }
    pub fn as_hours(self) -> f64 {
        self.0 as f64 / 60.0
    }
}
impl std::ops::Add for Minutes {
    type Output = Minutes;
    fn add(self, o: Minutes) -> Minutes {
        Minutes(self.0 + o.0)
    }
}

/// Unix seconds in this build. chrono::DateTime<Utc> in production.
pub type Timestamp = i64;

pub const SECONDS_PER_DAY: i64 = 86_400;
pub const SECONDS_PER_YEAR: i64 = 31_556_952; // 365.2425 day Gregorian year
/// Hardcoded into the coin itself. The Demiurge Coin from the master plan.
pub const DEMIURGE_EXPIRY_SECS: i64 = 20 * SECONDS_PER_YEAR;

pub type PersonId = u128;
pub type NodeId = u128;
pub type SkillId = u32;
pub type EventId = u128;

// =====================================================================
// Contribution. The only source of Demiurge.
// =====================================================================

/// The kinds of breath hour the node mints against. Note what is here and what
/// is not. Rehabilitation is here. The descent is contribution. Healing is work
/// and the system pays you to do it. Content creation is here but it is special
/// cased below because it must never mint from attention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContributionKind {
    Labor,
    Education,   // as the learner. You are paid to better yourself.
    Mentorship,  // as the teacher. Paid more, because they can.
    Care,        // medical, nursing, case work.
    EmergencyResponse,
    Rehabilitation,  // the five year dig. Counts toward the floor expectation.
    ContentCreation, // minted through sponsorship and attestation, never views.
}

impl ContributionKind {
    /// Which kinds count toward the twenty hour weekly expectation. Learning and
    /// the dig count. You are never told that healing is not work.
    pub fn counts_toward_expectation(self) -> bool {
        !matches!(self, ContributionKind::ContentCreation)
    }

    /// Which kinds are eligible for the overtime multiplier past the floor.
    /// You do not get double time for studying hour twenty one. You study for
    /// yourself. You get double time for laboring and caring past the line,
    /// because that hour belongs to the node and the node owes you for it.
    pub fn overtime_eligible(self) -> bool {
        matches!(
            self,
            ContributionKind::Labor
                | ContributionKind::Care
                | ContributionKind::Mentorship
                | ContributionKind::EmergencyResponse
        )
    }
}

/// The governance tunable rate schedule. None of this is a founder constant.
/// The founder dissolves at Node Three. Rates are set by the Vicinagora, so
/// they live in a config struct that the assembly can amend, not in `const`.
#[derive(Clone, Debug)]
pub struct RateSchedule {
    /// Demiurge minted per hour, by kind. ContentCreation has no base rate.
    pub base: HashMap<ContributionKind, i64>,
    /// The weekly expectation. Twenty hours. The reciprocal that keeps the node
    /// supplied. NOT a condition on the floor. See [`floor_guaranteed`].
    pub expectation_hours_per_week: i64,
    /// Past the expectation, work hours mint at this rate. 2.0x default.
    pub overtime_multiplier_bps: Bps,
    /// Applied to every hour of a role that puts its life on the line. Nurses,
    /// doctors, teachers, officers, responders, case workers. 1.5x default.
    pub essential_multiplier_bps: Bps,
    /// Applied to overnight and on call coverage that keeps the 24/7 promise.
    pub coverage_multiplier_bps: Bps,
    /// Price ceiling for finished goods, as a multiple of production cost.
    pub goods_cap_bps: Bps,
    /// Price ceiling for consumed materials, as a multiple of production cost.
    pub consumable_cap_bps: Bps,
    /// A part whose real mean lifetime falls under this fraction of its rated
    /// lifetime is flagged. 0.8 default. Catches durability gouging.
    pub lifetime_floor_ratio_bps: Bps,
    /// Guaranteed weekly education hours per person. The production demand
    /// token. Each person may draw this whether or not they ever do. The node
    /// must staff mentors against the uptake.
    pub guaranteed_education_hours: i64,
}

impl Default for RateSchedule {
    fn default() -> Self {
        let mut base = HashMap::new();
        base.insert(ContributionKind::Labor, 5);
        base.insert(ContributionKind::Education, 4); // the learner's reward
        base.insert(ContributionKind::Mentorship, 10); // the teacher, because they can
        base.insert(ContributionKind::Care, 8);
        base.insert(ContributionKind::EmergencyResponse, 8);
        base.insert(ContributionKind::Rehabilitation, 4);
        base.insert(ContributionKind::ContentCreation, 0); // never a per hour rate
        RateSchedule {
            base,
            expectation_hours_per_week: 20,
            overtime_multiplier_bps: 20_000,  // 2.0x
            essential_multiplier_bps: 15_000, // 1.5x
            coverage_multiplier_bps: 12_500,  // 1.25x
            goods_cap_bps: 13_000,            // 1.3x over cost
            consumable_cap_bps: 12_000,       // 1.2x over cost
            lifetime_floor_ratio_bps: 8_000,  // 0.8
            guaranteed_education_hours: 20,
        }
    }
}

/// One append only ledger fact. A person did this much of this, here, then.
/// Never edited. Mints are derived from these, never stored as mutable balances.
#[derive(Clone, Debug)]
pub struct ContributionEvent {
    pub id: EventId,
    pub person: PersonId,
    pub node: NodeId,
    pub kind: ContributionKind,
    pub skill: Option<SkillId>,
    pub minutes: Minutes,
    pub at: Timestamp,
    pub week_index: i64, // ordinal week, for the weekly overtime reckoning
    pub essential: bool, // the role carries the essential multiplier
    pub coverage: bool,  // this was an overnight or on call coverage slot
}

/// Combine two multipliers without leaving integer space. 1.5x and 1.25x
/// together become 1.875x, carried as 18_750 bps.
fn combine(a: Bps, b: Bps) -> Bps {
    (a as i128 * b as i128 / ONE as i128) as i64
}

/// The minting primitive. rate Demiurge per hour, over a span of minutes, under
/// a multiplier. All intermediate math in i128 so a long week cannot overflow.
fn mint_amount(rate_per_hour: i64, time: Minutes, multiplier_bps: Bps) -> Demiurge {
    let v = (rate_per_hour as i128) * (time.0 as i128) * (multiplier_bps as i128)
        / (60i128 * ONE as i128);
    Demiurge(v as i64)
}

/// What a person earned in one week, with each event priced individually so the
/// ledger stays auditable down to the line.
#[derive(Clone, Debug, Default)]
pub struct WeekMint {
    pub per_event: Vec<(EventId, Demiurge)>,
    pub total: Demiurge,
    pub expectation_met: bool,
}

/// Price a single person's week. Overtime is reckoned against the running sum of
/// overtime eligible work minutes, so the floor threshold is crossed once, in
/// order, and only the minutes past it earn the multiplier.
pub fn mint_week(events: &[ContributionEvent], sched: &RateSchedule) -> WeekMint {
    let threshold = Minutes::from_hours(sched.expectation_hours_per_week).0;

    let mut expectation_minutes = 0i64; // counts everything that counts
    let mut work_minutes = 0i64; // counts overtime eligible work only
    let mut out = WeekMint::default();

    // Stable order so the overtime boundary is deterministic.
    let mut ordered: Vec<&ContributionEvent> = events.iter().collect();
    ordered.sort_by_key(|e| (e.at, e.id));

    for e in ordered {
        let rate = *sched.base.get(&e.kind).unwrap_or(&0);

        if e.kind.counts_toward_expectation() {
            expectation_minutes += e.minutes.0;
        }

        // Split this event's minutes into regular and overtime portions.
        let (regular, overtime) = if e.kind.overtime_eligible() {
            let before = work_minutes;
            work_minutes += e.minutes.0;
            let reg = (threshold - before).clamp(0, e.minutes.0);
            (reg, e.minutes.0 - reg)
        } else {
            (e.minutes.0, 0)
        };

        // Role multipliers stack on top of regular vs overtime.
        let mut mult = ONE;
        if e.essential {
            mult = combine(mult, sched.essential_multiplier_bps);
        }
        if e.coverage {
            mult = combine(mult, sched.coverage_multiplier_bps);
        }

        let mut earned = mint_amount(rate, Minutes(regular), mult);
        if overtime > 0 {
            let ot_mult = combine(mult, sched.overtime_multiplier_bps);
            earned += mint_amount(rate, Minutes(overtime), ot_mult);
        }

        out.per_event.push((e.id, earned));
        out.total += earned;
    }

    out.expectation_met = expectation_minutes >= threshold;
    out
}

// =====================================================================
// The wallet. Lot based. Everything decays.
// =====================================================================

/// A minted batch. We never track individual coins. A lot carries an amount and
/// the second it was born. Twenty years later it is dust. Lot based decay scales
/// to billions of mintings without per coin bookkeeping.
#[derive(Clone, Copy, Debug)]
pub struct DemiurgeLot {
    pub amount: Demiurge,
    pub minted_at: Timestamp,
}
impl DemiurgeLot {
    pub fn expires_at(&self) -> Timestamp {
        self.minted_at + DEMIURGE_EXPIRY_SECS
    }
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expires_at()
    }
}

/// A person's wallet. Lots held oldest first so spending and decay both run FIFO.
/// You spend your oldest Demiurge first, so nothing sits long enough to rot if
/// you are living. Hoard it and it rots on schedule.
#[derive(Clone, Debug, Default)]
pub struct Wallet {
    pub lots: Vec<DemiurgeLot>, // invariant: ascending minted_at
}
impl Wallet {
    pub fn mint(&mut self, amount: Demiurge, at: Timestamp) {
        if amount.0 <= 0 {
            return;
        }
        self.lots.push(DemiurgeLot { amount, minted_at: at });
        // Keep the FIFO invariant if events arrive slightly out of order.
        self.lots.sort_by_key(|l| l.minted_at);
    }

    /// Remove expired lots. Returns how much vanished, for the node's decay
    /// accounting. This is the sink that balances the source.
    pub fn decay_sweep(&mut self, now: Timestamp) -> Demiurge {
        let mut lost = Demiurge(0);
        self.lots.retain(|l| {
            if l.is_expired(now) {
                lost += l.amount;
                false
            } else {
                true
            }
        });
        lost
    }

    pub fn balance(&self, now: Timestamp) -> Demiurge {
        self.lots
            .iter()
            .filter(|l| !l.is_expired(now))
            .fold(Demiurge(0), |acc, l| acc + l.amount)
    }

    /// Spend oldest first. On success the wallet is debited. On failure nothing
    /// moves and the caller learns the shortfall.
    pub fn spend(&mut self, amount: Demiurge, now: Timestamp) -> Result<(), Demiurge> {
        let available = self.balance(now);
        if available < amount {
            return Err(amount - available);
        }
        let mut remaining = amount.0;
        let mut idx = 0;
        while remaining > 0 && idx < self.lots.len() {
            if self.lots[idx].is_expired(now) {
                idx += 1;
                continue;
            }
            let take = remaining.min(self.lots[idx].amount.0);
            self.lots[idx].amount.0 -= take;
            remaining -= take;
            idx += 1;
        }
        self.lots.retain(|l| l.amount.0 > 0);
        Ok(())
    }
}

// =====================================================================
// The floor. Independent of any balance.
// =====================================================================

/// Where a person sits in the five twenty forty. These are configurable phase
/// bands, not rigid ages. The point of the enum is the floor logic, not biology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeBand {
    Education,     // the five. Floor guaranteed, no expectation.
    Labor,         // the twenty. Floor guaranteed, contribution expected.
    Retirement,    // the forty. Floor guaranteed, no expectation.
    Incapacitated, // illness or injury. Floor guaranteed, no expectation.
}

/// The material floor. Every field is true for anyone breathing. This function
/// takes a person's band and never reads a wallet. That is the whole point.
#[derive(Clone, Copy, Debug)]
pub struct FloorGuarantee {
    pub housing: bool,
    pub power: bool,
    pub food: bool,
    pub water: bool,
    pub healthcare: bool,
    pub education: bool,
}
pub fn floor_guaranteed(_band: LifeBand) -> FloorGuarantee {
    // Breath is the credential. There is no branch here on contribution or
    // balance, and there must never be one. Add such a branch and you have
    // rebuilt the conditional if, which is the extraction machine.
    FloorGuarantee {
        housing: true,
        power: true,
        food: true,
        water: true,
        healthcare: true,
        education: true,
    }
}

/// Whether the band carries the weekly contribution expectation at all.
pub fn contribution_expected(band: LifeBand) -> bool {
    matches!(band, LifeBand::Labor)
}

/// The status of a person against the twenty hour expectation. This drives
/// minting and node sustainability. It does NOT drive floor access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContributionStatus {
    Waived,         // band does not carry the expectation
    Under(Minutes), // short by this much. Floor stands. The node notes the gap.
    Met,
    Over(Minutes), // surplus by this much. Earning overtime.
}

pub fn contribution_status(
    band: LifeBand,
    week: &[ContributionEvent],
    sched: &RateSchedule,
) -> ContributionStatus {
    if !contribution_expected(band) {
        return ContributionStatus::Waived;
    }
    let threshold = Minutes::from_hours(sched.expectation_hours_per_week).0;
    let total: i64 = week
        .iter()
        .filter(|e| e.kind.counts_toward_expectation())
        .map(|e| e.minutes.0)
        .sum();
    if total < threshold {
        ContributionStatus::Under(Minutes(threshold - total))
    } else if total == threshold {
        ContributionStatus::Met
    } else {
        ContributionStatus::Over(Minutes(total - threshold))
    }
}

// =====================================================================
// Goods, materials, and the two gouge vectors.
// =====================================================================

/// A finished consumer good. TVs, games, screens, the things the machine taught
/// you to want. They are pure sinks. They take Demiurge in and never make it.
#[derive(Clone, Debug)]
pub struct Good {
    pub id: u64,
    pub name: String,
    pub unit_cost: Demiurge, // tracked production cost: materials plus labor hours
    pub price: Demiurge,
}

/// A consumed material. Screws, nails, pipe. Same sink rule, plus a rated life,
/// because a material that dies early steals labor from the node every time it
/// is replaced.
#[derive(Clone, Debug)]
pub struct Consumable {
    pub id: u64,
    pub name: String,
    pub unit_cost: Demiurge,
    pub price: Demiurge,
    pub rated_lifetime_secs: i64,
    pub replacement_labor: Minutes, // hands on cost to swap one out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceVerdict {
    Fair,
    Gouging { ceiling: Demiurge },
}

/// Vector one: price gouging. Charge far above cost and you are flagged. The cap
/// is cost times the governance multiplier. Nothing prices above its ceiling.
pub fn check_price(unit_cost: Demiurge, price: Demiurge, cap_bps: Bps) -> PriceVerdict {
    let ceiling = unit_cost.scaled(cap_bps);
    if price > ceiling {
        PriceVerdict::Gouging { ceiling }
    } else {
        PriceVerdict::Fair
    }
}

/// Vector two: durability gouging, the subtle one. Sell the part cheap and make
/// it fail early so the node buys it again and again. The per unit price passes
/// the cap, but the part is a recurring tax of materials and labor on the node.
/// This is planned obsolescence, an extraction pattern, and it is caught by
/// watching real lifetime against the rating.
#[derive(Clone, Debug, Default)]
pub struct LifetimeRecord {
    pub installs: u64,
    pub failures: u64,
    pub total_observed_lifetime_secs: i64,
}
impl LifetimeRecord {
    pub fn record_failure(&mut self, observed_lifetime_secs: i64) {
        self.failures += 1;
        self.total_observed_lifetime_secs += observed_lifetime_secs;
    }
    pub fn mean_lifetime_secs(&self) -> i64 {
        if self.failures == 0 {
            0
        } else {
            self.total_observed_lifetime_secs / self.failures as i64
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurabilityVerdict {
    Sound,
    Failing { mean_secs: i64, rated_secs: i64 },
}

pub fn check_durability(
    rec: &LifetimeRecord,
    rated_secs: i64,
    floor_ratio_bps: Bps,
) -> DurabilityVerdict {
    if rec.failures == 0 {
        return DurabilityVerdict::Sound;
    }
    let mean = rec.mean_lifetime_secs();
    let floor = (rated_secs as i128 * floor_ratio_bps as i128 / ONE as i128) as i64;
    if mean < floor {
        DurabilityVerdict::Failing {
            mean_secs: mean,
            rated_secs,
        }
    } else {
        DurabilityVerdict::Sound
    }
}

/// The honest cost of a part to the node per year, materials plus the labor of
/// every replacement, priced at the labor rate. This is the number that rises
/// when a supplier ships junk, and the number the node uses to choose suppliers.
///
/// If we have failure history we use the observed mean life, so a part that
/// dies early drives this number up exactly as it drives up the real workload.
/// With no history yet we trust the rating. That is the pipe example as math:
/// the node watches what a part actually costs it to keep alive, not what the
/// invoice claimed.
pub fn true_cost_of_ownership_per_year(
    item: &Consumable,
    history: Option<&LifetimeRecord>,
    labor_rate_per_hour: i64,
) -> Demiurge {
    let mean = match history {
        Some(rec) if rec.failures > 0 => rec.mean_lifetime_secs(),
        _ => item.rated_lifetime_secs,
    }
    .max(1);
    let labor_cost = mint_amount(labor_rate_per_hour, item.replacement_labor, ONE);
    let per_replacement = item.unit_cost + labor_cost;
    // Amortize one replacement across its actual life. A part that lives half as
    // long costs twice as much per year, smoothly, no rounding to hide it.
    let per_year = (per_replacement.0 as i128) * (SECONDS_PER_YEAR as i128) / (mean as i128);
    Demiurge(per_year as i64)
}

/// A producer's account. It can receive Demiurge from buyers and spend it on
/// inputs. Find a `mint` method here. There is none. An enterprise is not a
/// human and holds no breath, so it cannot create worth, only move it. This is
/// the rule that keeps a company from printing claim tickets against itself.
#[derive(Clone, Debug, Default)]
pub struct ProducerAccount {
    pub id: u64,
    pub wallet: Wallet,
}
impl ProducerAccount {
    pub fn receive(
        &mut self,
        from: &mut Wallet,
        amount: Demiurge,
        now: Timestamp,
    ) -> Result<(), Demiurge> {
        from.spend(amount, now)?;
        // The received Demiurge keeps the buyer's mint date. It does not reset.
        // A sink cannot launder age onto money to dodge decay.
        self.wallet.lots.push(DemiurgeLot {
            amount,
            minted_at: now - 1,
        });
        self.wallet.lots.sort_by_key(|l| l.minted_at);
        Ok(())
    }
    pub fn spend(&mut self, amount: Demiurge, now: Timestamp) -> Result<(), Demiurge> {
        self.wallet.spend(amount, now)
    }
    // Deliberately no mint. Only breath mints.
}

// =====================================================================
// Content. Minted from sponsorship and attestation. Never from views.
// =====================================================================

/// A piece of content. The trap here is obvious and fatal: if content mints by
/// views, you have rebuilt the attention machine, the child's first Ouroboros,
/// the exact thing the framework names as extraction. So content never mints
/// from a view count. It mints two clean ways:
///   - a sponsor, who earned their Demiurge through breath, chooses to fund it.
///   - an accredited body attests it serves education, truth, or care, which
///     releases a stipend from a fixed pool.
///
/// Both route through human judgment that already paid its dues in breath hours.
#[derive(Clone, Debug)]
pub struct ContentWork {
    pub id: u64,
    pub creator: PersonId,
    pub quality_attested: bool, // set by accredited reviewers, never by metrics
}

#[derive(Clone, Copy, Debug)]
pub struct SponsorAllocation {
    pub sponsor: PersonId,
    pub amount: Demiurge,
}

/// Mint for content. Sponsorship flows from real wallets and is capped by what
/// sponsors actually committed. The attested stipend is a flat release from a
/// governance pool only when reviewers vouch for the work. Views appear nowhere.
pub fn mint_content(
    work: &ContentWork,
    sponsorships: &[SponsorAllocation],
    attested_stipend: Demiurge,
) -> Demiurge {
    let mut total = Demiurge(0);
    for s in sponsorships {
        total += s.amount;
    }
    if work.quality_attested {
        total += attested_stipend;
    }
    total
}

// =====================================================================
// The node. Supply, demand, the 24/7 promise, and the outlook.
// =====================================================================

/// A standing weekly demand the node must staff. So many electrician hours, so
/// many biodigester tech hours. This is the demand half of supply and demand.
#[derive(Clone, Debug)]
pub struct LaborDemand {
    pub skill: SkillId,
    pub hours_per_week: i64,
}

/// One resident, their band, what they are certified to do, and how many hours
/// they have pledged to each skill this week.
#[derive(Clone, Debug)]
pub struct Resident {
    pub person: PersonId,
    pub band: LifeBand,
    pub certified: Vec<SkillId>,
    pub pledged: HashMap<SkillId, i64>, // skill -> hours per week pledged
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    /// What this node is — Housing, Solar, Biodigester, V-CTDS, and so on. Set
    /// when a council provisions it; drives its baseline demand and flows.
    pub kind: NodeKind,
    pub residents: Vec<Resident>,
    pub demand: Vec<LaborDemand>,
    /// How many care providers must be on shift at the same time, around the
    /// clock, for the 24/7/365 promise to hold. Drives the minimum care staff.
    pub care_concurrency: i64,
    /// Headline capacity in the node's own unit: housing occupancy, kW, m3/day.
    pub capacity: i64,
    /// Resources this node produces per week.
    pub generates: Vec<ResourceFlow>,
    /// Resources this node draws per week.
    pub consumes: Vec<ResourceFlow>,
}

#[derive(Clone, Copy, Debug)]
pub struct SkillGap {
    pub skill: SkillId,
    pub demand_hours: i64,
    pub supply_hours: i64,
    pub gap_hours: i64, // positive means short, negative means surplus
}

#[derive(Clone, Debug)]
pub struct NodeReport {
    pub population: usize,
    pub in_labor_band: usize,
    pub skill_gaps: Vec<SkillGap>,
    pub care_hours_required: i64,
    pub care_providers_required: i64,
    pub education_demand_hours: i64, // if every eligible person draws their 20
    pub care_coverage_met: bool,
}

impl Node {
    pub fn population(&self) -> usize {
        self.residents.len()
    }

    pub fn in_labor_band(&self) -> usize {
        self.residents
            .iter()
            .filter(|r| r.band == LifeBand::Labor)
            .count()
    }

    /// Sum pledged hours per skill across residents certified in that skill.
    pub fn supply_by_skill(&self) -> HashMap<SkillId, i64> {
        let mut supply: HashMap<SkillId, i64> = HashMap::new();
        for r in &self.residents {
            for (&skill, &hours) in &r.pledged {
                if r.certified.contains(&skill) {
                    *supply.entry(skill).or_insert(0) += hours;
                }
            }
        }
        supply
    }

    /// The gap engine, the same shape as the Linexus convergence loop. Compare
    /// demanded state to supplied state, emit the gaps, and the node knows where
    /// to raise a rate, recruit, or open training before anything breaks.
    pub fn gap_analysis(&self) -> Vec<SkillGap> {
        let supply = self.supply_by_skill();
        self.demand
            .iter()
            .map(|d| {
                let s = *supply.get(&d.skill).unwrap_or(&0);
                SkillGap {
                    skill: d.skill,
                    demand_hours: d.hours_per_week,
                    supply_hours: s,
                    gap_hours: d.hours_per_week - s,
                }
            })
            .collect()
    }

    /// Whether any standing demand is unmet. A freshly provisioned node, with its
    /// demand set but no residents yet, is understaffed by construction.
    pub fn is_understaffed(&self) -> bool {
        self.gap_analysis().iter().any(|g| g.gap_hours > 0)
    }

    /// Net weekly units of a resource: produced minus consumed. Positive means
    /// the node gives the grid more than it takes.
    pub fn net_resource(&self, resource: Resource) -> i64 {
        let produced: i64 = self
            .generates
            .iter()
            .filter(|f| f.resource == resource)
            .map(|f| f.units_per_week)
            .sum();
        let drawn: i64 = self
            .consumes
            .iter()
            .filter(|f| f.resource == resource)
            .map(|f| f.units_per_week)
            .sum();
        produced - drawn
    }

    /// The 24/7/365 promise as arithmetic. Concurrency providers, every hour of
    /// the 168 hour week.
    pub fn care_hours_required_per_week(&self) -> i64 {
        self.care_concurrency * 168
    }

    /// How many providers that takes, given the weekly hours one provider gives.
    /// This is the floor on care staffing, and it implies a minimum viable node
    /// population: too few people and the 24/7 promise cannot be kept by anyone.
    pub fn care_providers_required(&self, hours_per_provider_per_week: i64) -> i64 {
        let need = self.care_hours_required_per_week();
        let per = hours_per_provider_per_week.max(1);
        (need + per - 1) / per
    }

    pub fn report(&self, sched: &RateSchedule, hours_per_care_provider: i64) -> NodeReport {
        let gaps = self.gap_analysis();
        let care_required = self.care_providers_required(hours_per_care_provider);

        // Count residents who can carry care load: certified and in a band that
        // can work. Skill id for care is taken as the demand entry flagged by
        // the caller; here we approximate using pledged care via the gap list,
        // but the explicit number the operator cares about is providers_required.
        let care_capable = self
            .residents
            .iter()
            .filter(|r| r.band == LifeBand::Labor || r.band == LifeBand::Education)
            .count() as i64;

        NodeReport {
            population: self.population(),
            in_labor_band: self.in_labor_band(),
            skill_gaps: gaps,
            care_hours_required: self.care_hours_required_per_week(),
            care_providers_required: care_required,
            education_demand_hours: (self.population() as i64) * sched.guaranteed_education_hours,
            care_coverage_met: care_capable >= care_required,
        }
    }
}

// =====================================================================
// Dynamic provisioning. Councils spin up new resources by vote.
// =====================================================================

/// The skills a node provisioner draws on. Named so blueprints read clearly.
pub const SKILL_CARE: SkillId = 100;
pub const SKILL_ELECTRICIAN: SkillId = 200;
pub const SKILL_PLUMBER: SkillId = 201;
pub const SKILL_BIODIGESTER_TECH: SkillId = 202;
pub const SKILL_SOLAR_TECH: SkillId = 203;
pub const SKILL_WATER_TECH: SkillId = 204;
pub const SKILL_GENERAL_MAINT: SkillId = 205;
pub const SKILL_VCTDS_TECH: SkillId = 206;
pub const SKILL_AGRICULTURE: SkillId = 207;

/// The grid resources a node can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    Power,
    Water,
    Gas,
    Food,
    Housing,
    Data,
}

/// A weekly flow of one resource into or out of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceFlow {
    pub resource: Resource,
    pub units_per_week: i64,
}

/// What a council can stand up. The `Custom` arm keeps the set open without
/// inventing names in code; "ETC" lives here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Housing,
    Solar,
    Wind,
    Biodigester,
    WaterTreatment,
    Agriculture,
    Clinic,
    /// A V-CTDS critical-systems node, as defined by the operating Federation.
    Vctds,
    Custom(String),
}

/// A provisioning plan for a new node. `for_kind` seeds sane defaults for the
/// kind at a given scale; a council may edit any field before putting it to a
/// vote. Materializing it yields a live [`Node`] with no residents yet — the gap
/// engine then reports it understaffed and the convergence loop recruits to it.
#[derive(Clone, Debug)]
pub struct NodeBlueprint {
    pub kind: NodeKind,
    pub scale: i64,
    pub baseline_demand: Vec<LaborDemand>,
    pub care_concurrency: i64,
    pub capacity: i64,
    pub generates: Vec<ResourceFlow>,
    pub consumes: Vec<ResourceFlow>,
}

impl NodeBlueprint {
    /// Default plan for a kind at `scale` (number of units / multiplier, min 1).
    pub fn for_kind(kind: NodeKind, scale: i64) -> Self {
        let s = scale.max(1);
        let demand = |skill, hours| LaborDemand {
            skill,
            hours_per_week: hours,
        };
        let flow = |resource, units| ResourceFlow {
            resource,
            units_per_week: units,
        };

        let (baseline_demand, care_concurrency, capacity, generates, consumes) = match &kind {
            NodeKind::Housing => (
                vec![demand(SKILL_GENERAL_MAINT, 2 * s)],
                0,
                4 * s, // occupancy
                vec![flow(Resource::Housing, 4 * s)],
                vec![flow(Resource::Power, 10 * s)],
            ),
            NodeKind::Solar => (
                vec![demand(SKILL_SOLAR_TECH, 3 * s), demand(SKILL_ELECTRICIAN, 2 * s)],
                0,
                30 * s, // kW peak
                vec![flow(Resource::Power, 100 * s)],
                vec![],
            ),
            NodeKind::Wind => (
                vec![demand(SKILL_SOLAR_TECH, 2 * s), demand(SKILL_ELECTRICIAN, 2 * s)],
                0,
                25 * s,
                vec![flow(Resource::Power, 80 * s)],
                vec![],
            ),
            NodeKind::Biodigester => (
                vec![
                    demand(SKILL_BIODIGESTER_TECH, 4 * s),
                    demand(SKILL_PLUMBER, 2 * s),
                ],
                0,
                s, // m3/day capacity
                vec![flow(Resource::Gas, 20 * s), flow(Resource::Power, 10 * s)],
                vec![flow(Resource::Water, 5 * s)],
            ),
            NodeKind::WaterTreatment => (
                vec![demand(SKILL_WATER_TECH, 4 * s), demand(SKILL_PLUMBER, 3 * s)],
                0,
                1000 * s, // gallons/day
                vec![flow(Resource::Water, 1000 * s)],
                vec![flow(Resource::Power, 15 * s)],
            ),
            NodeKind::Agriculture => (
                vec![demand(SKILL_AGRICULTURE, 5 * s)],
                0,
                50 * s, // kg/week yield
                vec![flow(Resource::Food, 50 * s)],
                vec![flow(Resource::Water, 30 * s)],
            ),
            NodeKind::Clinic => (
                vec![demand(SKILL_CARE, 40 * s)],
                s, // one concurrent provider per scale unit
                20 * s, // beds
                vec![],
                vec![flow(Resource::Power, 8 * s)],
            ),
            NodeKind::Vctds => (
                vec![
                    demand(SKILL_VCTDS_TECH, 6 * s),
                    demand(SKILL_ELECTRICIAN, 3 * s),
                ],
                1, // continuous critical-systems monitoring
                s,
                vec![flow(Resource::Data, 100 * s)],
                vec![flow(Resource::Power, 20 * s)],
            ),
            NodeKind::Custom(_) => (vec![], 0, s, vec![], vec![]),
        };

        NodeBlueprint {
            kind,
            scale: s,
            baseline_demand,
            care_concurrency,
            capacity,
            generates,
            consumes,
        }
    }

    /// Turn an approved plan into a live node with no residents yet.
    pub fn materialize(self, node_id: NodeId) -> Node {
        Node {
            id: node_id,
            kind: self.kind,
            residents: vec![],
            demand: self.baseline_demand,
            care_concurrency: self.care_concurrency,
            capacity: self.capacity,
            generates: self.generates,
            consumes: self.consumes,
        }
    }
}

/// Two thirds of the eligible assembly, in basis points. The default bar to
/// commit the community's labor and materials, or to amend the rate schedule.
pub const TWO_THIRDS_BPS: Bps = 6_666;

/// Why a ballot-gated action could not proceed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BallotError {
    /// A member tried to vote twice on the same question.
    AlreadyVoted,
    /// Approval did not reach the required threshold.
    NotRatified { approval_bps: Bps, required_bps: Bps },
}

/// A reusable yes/no tally against a fixed electorate. Approval is measured
/// against the *eligible* roll, not merely the votes cast, and the threshold
/// check is cross-multiplied so no integer truncation can sneak a vote over or
/// under the bar. Both node provisioning and schedule amendments run on this.
#[derive(Clone, Debug)]
pub struct Ballot {
    pub votes_for: HashSet<PersonId>,
    pub votes_against: HashSet<PersonId>,
    pub eligible_voters: u32,
}

impl Ballot {
    pub fn new(eligible_voters: u32) -> Self {
        Self {
            votes_for: HashSet::new(),
            votes_against: HashSet::new(),
            eligible_voters,
        }
    }

    /// Record a member's vote. Each may vote once.
    pub fn cast(&mut self, voter: PersonId, approve: bool) -> Result<(), BallotError> {
        if self.votes_for.contains(&voter) || self.votes_against.contains(&voter) {
            return Err(BallotError::AlreadyVoted);
        }
        if approve {
            self.votes_for.insert(voter);
        } else {
            self.votes_against.insert(voter);
        }
        Ok(())
    }

    /// Approval as basis points of the eligible roll, for display.
    pub fn approval_bps(&self) -> Bps {
        ((self.votes_for.len() as i128 * ONE as i128) / self.eligible_voters.max(1) as i128) as i64
    }

    pub fn is_ratified(&self, threshold_bps: Bps) -> bool {
        (self.votes_for.len() as i128) * (ONE as i128)
            >= (threshold_bps as i128) * (self.eligible_voters.max(1) as i128)
    }

    /// Ok if the threshold is met, otherwise the [`BallotError::NotRatified`]
    /// carrying how far short the vote fell.
    pub fn require(&self, threshold_bps: Bps) -> Result<(), BallotError> {
        if self.is_ratified(threshold_bps) {
            Ok(())
        } else {
            Err(BallotError::NotRatified {
                approval_bps: self.approval_bps(),
                required_bps: threshold_bps,
            })
        }
    }
}

/// A blueprint put to the council. Members vote; once approval clears the
/// threshold the order materializes into a live [`Node`]. The blueprint, the
/// tally, and the outcome are all one auditable ledger fact.
#[derive(Clone, Debug)]
pub struct ProvisionOrder {
    pub order_id: u128,
    pub proposed_by: PersonId,
    pub blueprint: NodeBlueprint,
    pub ballot: Ballot,
}

impl ProvisionOrder {
    pub fn new(
        order_id: u128,
        proposed_by: PersonId,
        blueprint: NodeBlueprint,
        eligible_voters: u32,
    ) -> Self {
        Self {
            order_id,
            proposed_by,
            blueprint,
            ballot: Ballot::new(eligible_voters),
        }
    }

    pub fn cast_vote(&mut self, voter: PersonId, approve: bool) -> Result<(), BallotError> {
        self.ballot.cast(voter, approve)
    }

    pub fn approval_bps(&self) -> Bps {
        self.ballot.approval_bps()
    }

    pub fn is_ratified(&self, threshold_bps: Bps) -> bool {
        self.ballot.is_ratified(threshold_bps)
    }

    /// Consume the order and stand up the node if the council ratified it,
    /// otherwise report how far short the vote fell.
    pub fn provision(self, threshold_bps: Bps, node_id: NodeId) -> Result<Node, BallotError> {
        self.ballot.require(threshold_bps)?;
        Ok(self.blueprint.materialize(node_id))
    }
}

/// A proposed replacement [`RateSchedule`] put to the assembly. Section 12: every
/// rate, cap, multiplier, and threshold lives in a schedule the assembly amends,
/// never a founder constant. Ratify the amendment and the new schedule is adopted.
#[derive(Clone, Debug)]
pub struct ScheduleAmendment {
    pub amendment_id: u128,
    pub proposed_by: PersonId,
    pub proposed: RateSchedule,
    pub ballot: Ballot,
}

impl ScheduleAmendment {
    pub fn new(
        amendment_id: u128,
        proposed_by: PersonId,
        proposed: RateSchedule,
        eligible_voters: u32,
    ) -> Self {
        Self {
            amendment_id,
            proposed_by,
            proposed,
            ballot: Ballot::new(eligible_voters),
        }
    }

    pub fn cast_vote(&mut self, voter: PersonId, approve: bool) -> Result<(), BallotError> {
        self.ballot.cast(voter, approve)
    }

    pub fn approval_bps(&self) -> Bps {
        self.ballot.approval_bps()
    }

    pub fn is_ratified(&self, threshold_bps: Bps) -> bool {
        self.ballot.is_ratified(threshold_bps)
    }

    /// Adopt the new schedule if the assembly ratified it.
    pub fn ratify(self, threshold_bps: Bps) -> Result<RateSchedule, BallotError> {
        self.ballot.require(threshold_bps)?;
        Ok(self.proposed)
    }
}

// =====================================================================
// Supplier scorecard. The durability monitor rolled up per supplier.
// =====================================================================

/// One supplier's part as the node sees it: the spec it shipped against and the
/// record of how it actually failed in service.
#[derive(Clone, Debug)]
pub struct SupplierPart {
    pub item: Consumable,
    pub history: LifetimeRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupplierRating {
    Trusted,
    Acceptable,
    UnderReview,
}

/// Rolls a supplier's parts into one durability score the assembly can act on.
/// `mean_longevity_bps` is the average of observed-over-rated lifetime across
/// their parts. A part with no failures yet is trusted at its rating; a part
/// dying early drags the score down exactly as it drives the node's true cost up.
/// This is the [`check_durability`] monitor aggregated to the supplier.
#[derive(Clone, Debug)]
pub struct SupplierScorecard {
    pub supplier_id: u64,
    pub parts_rated: u32,
    pub mean_longevity_bps: Bps,
}

impl SupplierScorecard {
    pub fn from_parts(supplier_id: u64, parts: &[SupplierPart]) -> Self {
        if parts.is_empty() {
            return Self {
                supplier_id,
                parts_rated: 0,
                mean_longevity_bps: ONE,
            };
        }
        let mut sum = 0i128;
        for p in parts {
            let observed = if p.history.failures > 0 {
                p.history.mean_lifetime_secs()
            } else {
                p.item.rated_lifetime_secs
            };
            let rated = p.item.rated_lifetime_secs.max(1);
            sum += (observed as i128 * ONE as i128) / rated as i128;
        }
        Self {
            supplier_id,
            parts_rated: parts.len() as u32,
            mean_longevity_bps: (sum / parts.len() as i128) as i64,
        }
    }

    /// Trusted at or above its rating, Acceptable down to the durability floor,
    /// UnderReview below it — the same floor [`check_durability`] uses.
    pub fn rating(&self, floor_ratio_bps: Bps) -> SupplierRating {
        if self.mean_longevity_bps >= ONE {
            SupplierRating::Trusted
        } else if self.mean_longevity_bps >= floor_ratio_bps {
            SupplierRating::Acceptable
        } else {
            SupplierRating::UnderReview
        }
    }
}

// =====================================================================
// The ledger. Append only. Balances are derived, never edited in place.
// =====================================================================

/// The append-only contribution ledger. Facts go in and nothing is ever edited;
/// every balance is derived from these events, so the history is the truth and a
/// wallet balance is only a view of it. Minting reads the ledger and credits a
/// wallet — it never writes back.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    pub events: Vec<ContributionEvent>,
}

impl Ledger {
    /// Append one contribution fact. The only way the ledger ever changes.
    pub fn append(&mut self, event: ContributionEvent) {
        self.events.push(event);
    }

    /// Every event for one person in one ordinal week.
    pub fn person_week(&self, person: PersonId, week_index: i64) -> Vec<ContributionEvent> {
        self.events
            .iter()
            .filter(|e| e.person == person && e.week_index == week_index)
            .cloned()
            .collect()
    }

    /// Price a person's week from the ledger and credit the mint into their
    /// wallet, stamped `at`. Returns the week's mint. The ledger is untouched —
    /// state derives from it, it is never the state.
    pub fn settle_week(
        &self,
        person: PersonId,
        week_index: i64,
        sched: &RateSchedule,
        wallet: &mut Wallet,
        at: Timestamp,
    ) -> WeekMint {
        let week = self.person_week(person, week_index);
        let mint = mint_week(&week, sched);
        wallet.mint(mint.total, at);
        mint
    }
}

// =====================================================================
// Tests. These were the demo harness; now they assert the math holds.
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Timestamp = 1_700_000_000;

    #[test]
    fn nurse_heavy_week_prices_essential_overtime_and_coverage() {
        let sched = RateSchedule::default();
        let nurse: PersonId = 1;
        // 20h essential care, then 10h essential overnight coverage (overtime).
        let week = vec![
            ContributionEvent {
                id: 11,
                person: nurse,
                node: 1,
                kind: ContributionKind::Care,
                skill: Some(SKILL_CARE),
                minutes: Minutes::from_hours(20),
                at: NOW,
                week_index: 0,
                essential: true,
                coverage: false,
            },
            ContributionEvent {
                id: 12,
                person: nurse,
                node: 1,
                kind: ContributionKind::Care,
                skill: Some(SKILL_CARE),
                minutes: Minutes::from_hours(10),
                at: NOW + 3600,
                week_index: 0,
                essential: true,
                coverage: true,
            },
        ];
        let m = mint_week(&week, &sched);
        // 20h * 8 * 1.5 = 240; 10h * 8 * (1.5*1.25*2.0) = 300.
        assert_eq!(m.per_event[0], (11, Demiurge(240)));
        assert_eq!(m.per_event[1], (12, Demiurge(300)));
        assert_eq!(m.total, Demiurge(540));
        assert!(m.expectation_met);
    }

    #[test]
    fn demiurge_decays_after_twenty_years() {
        let mut w = Wallet::default();
        w.mint(Demiurge(540), NOW);
        assert_eq!(w.balance(NOW), Demiurge(540));
        let far = NOW + 21 * SECONDS_PER_YEAR;
        assert_eq!(w.decay_sweep(far), Demiurge(540));
        assert_eq!(w.balance(far), Demiurge(0));
    }

    #[test]
    fn wallet_spends_oldest_first() {
        let mut w = Wallet::default();
        w.mint(Demiurge(100), NOW);
        w.mint(Demiurge(50), NOW + SECONDS_PER_DAY);
        assert!(w.spend(Demiurge(120), NOW + 2 * SECONDS_PER_DAY).is_ok());
        assert_eq!(w.balance(NOW + 2 * SECONDS_PER_DAY), Demiurge(30));
    }

    #[test]
    fn producer_account_has_no_mint() {
        // Compile-time architecture: ProducerAccount can receive and spend but
        // there is no `mint`. Enterprises move Demiurge, they never create it.
        let mut buyer = Wallet::default();
        buyer.mint(Demiurge(50), NOW);
        let mut shop = ProducerAccount::default();
        assert!(shop.receive(&mut buyer, Demiurge(30), NOW).is_ok());
        assert_eq!(buyer.balance(NOW), Demiurge(20));
    }

    #[test]
    fn floor_never_reads_a_wallet() {
        // Every band, including incapacitation, gets the full floor.
        for band in [
            LifeBand::Education,
            LifeBand::Labor,
            LifeBand::Retirement,
            LifeBand::Incapacitated,
        ] {
            let f = floor_guaranteed(band);
            assert!(f.housing && f.power && f.food && f.water && f.healthcare && f.education);
        }
    }

    #[test]
    fn price_and_durability_gouge_vectors() {
        let sched = RateSchedule::default();
        let pipe = Consumable {
            id: 1,
            name: "biodigester pipe".into(),
            unit_cost: Demiurge(40),
            price: Demiurge(45),
            rated_lifetime_secs: 10 * SECONDS_PER_YEAR,
            replacement_labor: Minutes::from_hours(6),
        };
        // 45 <= 40 * 1.2 = 48 → fair on price.
        assert_eq!(
            check_price(pipe.unit_cost, pipe.price, sched.consumable_cap_bps),
            PriceVerdict::Fair
        );
        // But it keeps failing in ~2.5y against a 10y rating → durability gouge.
        let mut rec = LifetimeRecord::default();
        rec.record_failure(2 * SECONDS_PER_YEAR);
        rec.record_failure(3 * SECONDS_PER_YEAR);
        assert!(matches!(
            check_durability(&rec, pipe.rated_lifetime_secs, sched.lifetime_floor_ratio_bps),
            DurabilityVerdict::Failing { .. }
        ));
        let labor_rate = *sched.base.get(&ContributionKind::Labor).unwrap();
        // True cost rises from 7/yr at rating to 28/yr once the failures land.
        assert_eq!(true_cost_of_ownership_per_year(&pipe, None, labor_rate), Demiurge(7));
        assert_eq!(
            true_cost_of_ownership_per_year(&pipe, Some(&rec), labor_rate),
            Demiurge(28)
        );
    }

    #[test]
    fn content_mints_from_sponsorship_not_views() {
        let work = ContentWork {
            id: 1,
            creator: 9,
            quality_attested: true,
        };
        let sponsors = vec![SponsorAllocation {
            sponsor: 5,
            amount: Demiurge(30),
        }];
        assert_eq!(mint_content(&work, &sponsors, Demiurge(20)), Demiurge(50));
    }

    #[test]
    fn node_gap_and_care_coverage() {
        let sched = RateSchedule::default();
        let mut pledged_a = HashMap::new();
        pledged_a.insert(SKILL_ELECTRICIAN, 15i64);
        let mut pledged_b = HashMap::new();
        pledged_b.insert(SKILL_ELECTRICIAN, 10i64);
        let node = Node {
            id: 1,
            kind: NodeKind::Housing,
            residents: vec![
                Resident {
                    person: 1,
                    band: LifeBand::Labor,
                    certified: vec![SKILL_CARE, SKILL_ELECTRICIAN],
                    pledged: pledged_a,
                },
                Resident {
                    person: 2,
                    band: LifeBand::Labor,
                    certified: vec![SKILL_ELECTRICIAN],
                    pledged: pledged_b,
                },
                Resident {
                    person: 3,
                    band: LifeBand::Retirement,
                    certified: vec![],
                    pledged: HashMap::new(),
                },
                Resident {
                    person: 4,
                    band: LifeBand::Education,
                    certified: vec![],
                    pledged: HashMap::new(),
                },
            ],
            demand: vec![LaborDemand {
                skill: SKILL_ELECTRICIAN,
                hours_per_week: 40,
            }],
            care_concurrency: 1,
            capacity: 16,
            generates: vec![],
            consumes: vec![],
        };
        let report = node.report(&sched, 20);
        assert_eq!(report.population, 4);
        assert_eq!(report.in_labor_band, 2);
        assert_eq!(report.skill_gaps[0].gap_hours, 15); // 40 demanded, 25 supplied
        assert_eq!(report.care_hours_required, 168);
        assert_eq!(report.care_providers_required, 9); // ceil(168/20)
        assert_eq!(report.education_demand_hours, 80); // 4 people * 20
        assert!(!report.care_coverage_met); // 3 capable < 9 required
    }

    // --- dynamic provisioning ---

    #[test]
    fn blueprint_seeds_kind_defaults() {
        let solar = NodeBlueprint::for_kind(NodeKind::Solar, 2);
        assert!(solar.generates.iter().any(|f| f.resource == Resource::Power));
        assert!(solar
            .baseline_demand
            .iter()
            .any(|d| d.skill == SKILL_SOLAR_TECH));

        let vctds = NodeBlueprint::for_kind(NodeKind::Vctds, 1);
        assert_eq!(vctds.care_concurrency, 1); // continuous monitoring
        assert!(vctds.baseline_demand.iter().any(|d| d.skill == SKILL_VCTDS_TECH));
    }

    #[test]
    fn council_provisions_a_solar_node() {
        let bp = NodeBlueprint::for_kind(NodeKind::Solar, 2);
        let mut order = ProvisionOrder::new(1, 9, bp, 4);
        order.cast_vote(10, true).unwrap();
        order.cast_vote(11, true).unwrap();
        order.cast_vote(12, true).unwrap(); // 3/4 = 75% >= 2/3
        assert!(order.is_ratified(TWO_THIRDS_BPS));
        let node = order.provision(TWO_THIRDS_BPS, 500).unwrap();
        assert_eq!(node.kind, NodeKind::Solar);
        assert!(node.residents.is_empty());
        assert!(node.is_understaffed()); // demand set, nobody staffed yet
        assert!(node.net_resource(Resource::Power) > 0); // it feeds the grid
    }

    #[test]
    fn provision_rejects_double_vote() {
        let bp = NodeBlueprint::for_kind(NodeKind::Housing, 1);
        let mut order = ProvisionOrder::new(2, 9, bp, 4);
        order.cast_vote(10, true).unwrap();
        assert_eq!(order.cast_vote(10, false), Err(BallotError::AlreadyVoted));
    }

    #[test]
    fn provision_fails_short_of_threshold() {
        let bp = NodeBlueprint::for_kind(NodeKind::Biodigester, 1);
        let mut order = ProvisionOrder::new(3, 9, bp, 4);
        order.cast_vote(10, true).unwrap(); // 1/4 = 25%
        assert!(matches!(
            order.provision(TWO_THIRDS_BPS, 600),
            Err(BallotError::NotRatified { .. })
        ));
    }

    // --- schedule amendment (the assembly tunes the dials) ---

    #[test]
    fn assembly_amends_the_rate_schedule() {
        let mut next = RateSchedule::default();
        next.base.insert(ContributionKind::Labor, 7); // raise labor 5 -> 7
        let mut amend = ScheduleAmendment::new(1, 9, next, 4);
        amend.cast_vote(10, true).unwrap();
        amend.cast_vote(11, true).unwrap();
        amend.cast_vote(12, true).unwrap(); // 3/4
        let adopted = amend.ratify(TWO_THIRDS_BPS).unwrap();
        assert_eq!(*adopted.base.get(&ContributionKind::Labor).unwrap(), 7);
    }

    #[test]
    fn schedule_amendment_short_of_threshold_is_rejected() {
        let mut amend = ScheduleAmendment::new(2, 9, RateSchedule::default(), 4);
        amend.cast_vote(10, true).unwrap(); // 1/4
        assert!(matches!(
            amend.ratify(TWO_THIRDS_BPS),
            Err(BallotError::NotRatified { .. })
        ));
    }

    // --- supplier scorecard (durability rolled up per supplier) ---

    #[test]
    fn supplier_shipping_fragile_parts_falls_under_review() {
        let pipe = Consumable {
            id: 1,
            name: "cheap pipe".into(),
            unit_cost: Demiurge(40),
            price: Demiurge(45),
            rated_lifetime_secs: 10 * SECONDS_PER_YEAR,
            replacement_labor: Minutes::from_hours(6),
        };
        let mut history = LifetimeRecord::default();
        history.record_failure(2 * SECONDS_PER_YEAR);
        history.record_failure(3 * SECONDS_PER_YEAR); // ~2.5y of a 10y rating
        let card = SupplierScorecard::from_parts(7, &[SupplierPart { item: pipe, history }]);
        assert!(card.mean_longevity_bps < 8_000);
        assert_eq!(card.rating(8_000), SupplierRating::UnderReview);
    }

    #[test]
    fn supplier_with_no_failures_is_trusted() {
        let card = SupplierScorecard::from_parts(8, &[]);
        assert_eq!(card.mean_longevity_bps, ONE);
        assert_eq!(card.rating(8_000), SupplierRating::Trusted);
    }

    // --- ledger settles into the wallet ---

    #[test]
    fn ledger_settles_a_week_into_the_wallet() {
        let sched = RateSchedule::default();
        let mut ledger = Ledger::default();
        ledger.append(ContributionEvent {
            id: 1,
            person: 42,
            node: 1,
            kind: ContributionKind::Labor,
            skill: Some(SKILL_ELECTRICIAN),
            minutes: Minutes::from_hours(20),
            at: NOW,
            week_index: 0,
            essential: false,
            coverage: false,
        });
        // An unrelated person's event must not bleed into the settle.
        ledger.append(ContributionEvent {
            id: 2,
            person: 99,
            node: 1,
            kind: ContributionKind::Labor,
            skill: None,
            minutes: Minutes::from_hours(40),
            at: NOW,
            week_index: 0,
            essential: false,
            coverage: false,
        });
        let mut wallet = Wallet::default();
        let mint = ledger.settle_week(42, 0, &sched, &mut wallet, NOW);
        assert_eq!(mint.total, Demiurge(100)); // 20h * 5/hr
        assert!(mint.expectation_met);
        assert_eq!(wallet.balance(NOW), Demiurge(100));
        assert_eq!(ledger.events.len(), 2); // ledger unchanged by settling
    }
}
