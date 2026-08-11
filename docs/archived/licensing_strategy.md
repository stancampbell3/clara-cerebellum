# Clara / CAWS Licensing Strategy

**Seashell Analytics, LLC | Confidential**
**Status:** Planning — pre-term sheet

---

## License Structure

### Type: Field-of-Use Exclusive

Clara / CAWS will be offered under a **field-of-use exclusive license**, defined broadly as **media content recommendation**. This includes music, video, podcast, audiobook, and related content discovery applications.

Seashell retains the right to license in all other domains independently. Priority vertical targets for parallel licensing include healthcare (clinical decision support), financial services (risk and compliance reasoning), and manufacturing (process fault diagnosis). These are to be pursued separately and positioned distinctly from the media use case.

**Rationale:** Field-of-use exclusivity commands a meaningful premium over non-exclusive while preserving Seashell's total addressable market. Defining the field as media broadly (rather than music only) increases the value to the licensee and justifies a higher fee while still leaving the majority of addressable industries open.

---

## Fee Structure

A three-component hybrid:

### 1. Upfront License Fee
A modest lump-sum payment at signing, establishing the IP relationship and providing immediate return. Sized to reflect the prototype/PPA stage rather than a proven production deployment. Specific figure to be determined from comparables analysis.

### 2. Milestone Payments
Payments triggered by defined deployment events, for example:
- Successful integration into a staging or test recommendation pipeline
- First production deployment (any region)
- Reaching a defined active user threshold (e.g., 10M, 50M, 100M users served by the CAWS reasoning layer)
- Renewal at defined intervals (e.g., year 3, year 5)

Milestone structure aligns incentives and is more auditable than running royalties alone.

### 3. Running Royalty with Cap and Floor
A per-query or revenue-percentage royalty on usage of the CAWS reasoning layer in production, subject to:
- **Floor:** Minimum annual payment regardless of usage, protecting Seashell's return if the licensee deploys minimally or delays rollout
- **Cap:** Maximum annual payment, providing the licensee with cost predictability at scale and reducing resistance to signing

Specific rates and cap/floor figures to be determined from comparables analysis and valuation modeling.

---

## Initial Licensing Position: Key Protective Terms

The following terms represent Seashell's initial negotiating position and should be included in any term sheet or licensing proposal.

### Improvement Rights and Grant-Back
Any improvements, extensions, or derivative works the licensee makes to the core CAWS algorithm or evaluate extension function are owned by Seashell Analytics. The licensee receives a perpetual, royalty-free license to use those improvements within the licensed field of use. This protects the integrity of the IP and ensures Seashell can incorporate improvements into future licensing.

### Internal Use Only / No Sublicensing
The license is limited to internal implementation within the licensee's own products and services. The licensee may not sublicense, resell, or embed CAWS in products or services offered to third parties without a separate written agreement with Seashell.

### Patent Prosecution Cooperation
The licensee agrees to cooperate with and not oppose Seashell's patent prosecution through issuance of a full utility patent. This includes providing commercially reasonable assistance if prior art challenges arise and refraining from filing inter partes review (IPR) or other post-grant challenges against the licensed patents.

### Audit Rights
For the duration of any royalty-bearing period, Seashell retains the right to audit the licensee's usage records no more than once per calendar year, with reasonable advance notice. Audit costs are borne by Seashell unless an underpayment exceeding a defined threshold (e.g., 5%) is found, in which case the licensee bears audit costs.

### Termination for Non-Use
If the licensee has not made commercially reasonable efforts to deploy the licensed technology within a defined period from signing (e.g., 24 months), Seashell may terminate the exclusive grant (while allowing the licensee to retain a non-exclusive license) or renegotiate exclusivity terms.

### Survival of Confidentiality
All technical disclosures made during negotiation and under the license agreement are subject to mutual confidentiality obligations that survive termination of the agreement. This is particularly important given that the full utility patent application is not yet filed at the time of initial licensing discussions.

### Most-Favored Licensee (Optional / Negotiating Chip)
If Seashell subsequently licenses CAWS in the same field of use to another party at more favorable terms, the original licensee may elect to adopt those terms. Include as a negotiating chip to reduce resistance to field-of-use exclusivity concerns.

---

## Pre-Negotiation Preparation

### 1. File Full Utility Patent Application
The 12-month PPA priority window must be tracked. Filing the utility application before or concurrent with term sheet exchange substantially strengthens negotiating position and protects against novelty challenges during due diligence.

### 2. Engage IP Licensing Counsel
Retain an attorney with software patent licensing experience and familiarity with big-tech in-licensing deals. All term sheets and agreement drafts should be reviewed by counsel before exchange.

### 3. Establish Valuation Basis
Build a comparables-based valuation model drawing on:
- Comparable AI/ML patent licensing deals (public filings, reported deals)
- Estimated engineering cost for Amazon to develop an equivalent system independently
- Projection of value added at Amazon's scale (per-query reasoning cost vs. value of explanation output)

See comparables analysis (in progress).

### 4. Define Walk-Away Position
Establish minimum acceptable terms before the first meeting. At minimum, a non-exclusive license with modest upfront fee and milestone payments would establish a reference deal for future licensees even if exclusivity cannot be negotiated.

### 5. Control the Starting Frame
Prepare a term sheet skeleton before any substantive meeting with Amazon. The party that tables the first draft controls the framing. This should be drafted with counsel once comparables are complete.

---

## Parallel Licensing Targets (Future)

The following domains are reserved for separate licensing efforts, positioned independently from the media use case:

- **Healthcare:** Clinical decision support, diagnostic reasoning under incomplete data
- **Financial Services:** Regulatory compliance reasoning, risk explainability (relevant to model governance requirements)
- **Manufacturing:** Process fault diagnosis, predictive maintenance with auditability requirements

---

---

## Comparables Analysis

*Researched May 2026. Deal-level financial terms for big-tech patent in-licensing are rarely public; the figures below are drawn from industry benchmarks, market reports, and reported deal structures. They establish a defensible range, not a fixed number.*

### Market Context

**Explainable AI market:** $7.79 billion in 2024, projected $21-$58 billion by 2030-2035 depending on the analyst, growing at roughly 15-21% CAGR. This is the market Clara competes in. It validates the size of the opportunity and supports a premium on IP that addresses the problem natively rather than post-hoc.

**Neurosymbolic AI:** The sector is attracting meaningful venture investment. Kognitos raised a $25M Series B in June 2025 for a neurosymbolic automation platform. The sub-market is growing because traceability and explainability are increasingly required in regulated and high-stakes domains — exactly the positioning Clara targets.

**AI patent licensing growth:** Licensing fees for AI-related patents have increased approximately 15% annually since 2020. The overall patent licensing market was $165 billion in 2024, projected to $355 billion by 2033.

### Competitive Signals That Strengthen Clara's Position

**The McKinsey gap:** A 2024 McKinsey study found that 40% of enterprises identify explainability as a key risk in adopting generative AI, but only 17% are actively working on mitigation. This is the exact gap Clara fills, and it exists inside Amazon as much as anywhere else.

**Big tech is filing XAI patents:** Microsoft has filed patents for model explanation and output validation tools. Oracle, Amazon, and Boeing have also filed explainability patents. This validates the market need and confirms that Amazon's own engineering leadership has identified this problem as worth investing in. It also means they will scrutinize Clara's claims carefully — which is why the formally grounded, neurosymbolic approach of CAWS is a differentiator: their filings address post-hoc attribution; Clara's PPA covers native reasoning transparency through a categorically different mechanism.

**Apple acquired DarwinAI (March 2024):** Apple's acquisition of the XAI startup DarwinAI signals that big tech is willing to acquire outright rather than build. Licensing is a lower-friction path to the same outcome for Amazon. No acquisition price was publicly disclosed, but the deal confirms strategic appetite for XAI IP at the major platform level.

### Benchmark Deal Structures

The following structures represent the relevant range for an early-stage AI/software patent license to a large technology company.

**Royalty rates:** Software patent licenses typically run 2-7% of net revenue attributable to the licensed technology. Early-stage and foundational algorithm licenses (where the licensee bears implementation cost) tend toward the lower end, 1-3%, offset by upfront and milestone payments. IBM structures AI licensing across percentage-based royalties, fixed fees, and hybrid models depending on the use case.

**Upfront fees:** Over 75% of patent licenses include an upfront payment. For a prototype/PPA-stage deal, the upfront fee reflects IP value rather than deployment value, and typically ranges from $50K to $500K for software. A higher upfront fee can be exchanged for a lower ongoing royalty rate.

**Milestone payments:** Milestone structures for software typically include gates at: (1) integration into a test or staging environment, (2) first production deployment, (3) scale thresholds (user counts or query volumes), and (4) periodic renewal. Individual milestone payments in technology licensing range from $50K to $1M+ depending on the significance of the gate.

**Example comparable structure (software/AI algorithm license):**
- $100K-$250K upfront at signing
- $250K-$500K milestone at production deployment
- $500K-$1M milestone at defined user scale threshold
- 2-3% running royalty on attributable revenue, with annual floor ($250K-$500K) and cap ($5M-$10M)

### Recommended Starting Position for Clara

Given the prototype/PPA stage, the size of the licensee, the field-of-use exclusivity being granted across all of media, and the differentiated technical approach:

| Component | Recommended Opening Position |
|---|---|
| Upfront fee | $250,000 |
| Milestone: production deployment | $500,000 |
| Milestone: 25M users served by CAWS layer | $750,000 |
| Milestone: 100M users served by CAWS layer | $1,500,000 |
| Running royalty | 2.5% of revenue attributable to CAWS-enabled features |
| Annual floor (post-deployment) | $500,000 |
| Annual cap | $8,000,000 |

These figures are an opening position, not a floor. Walk-away minimums should be established separately with counsel before any meeting.

### Key Valuation Arguments to Prepare

1. **Build vs. buy:** Estimate Amazon's internal cost to develop an equivalent neurosymbolic reasoning layer from scratch, including research, engineering, and time-to-market. A conservative estimate (2-3 senior AI researchers + engineering team for 18-24 months) easily exceeds the upfront and milestone totals.

2. **Market growth:** The XAI market is growing at 18% CAGR. First-mover exclusivity in media recommendation has compounding value.

3. **Regulatory tailwinds:** The EU AI Act and emerging US AI transparency frameworks create compliance obligations that CAWS is uniquely positioned to satisfy by design. Licensing now is cheaper than retrofitting later.

4. **The McKinsey gap:** 40% of enterprises recognize the explainability risk; 23% are not addressing it. Clara directly targets that unaddressed 23%.

---

## Open Items

- [x] Comparables analysis: AI/ML patent licensing deal benchmarks
- [ ] Valuation model: refine figures above with counsel and Amazon-specific revenue estimates
- [ ] Utility patent application filing timeline
- [ ] IP licensing counsel engagement
- [ ] Term sheet skeleton (post-comparables)
- [ ] Define field-of-use boundary language precisely (avoid ambiguity between "media recommendation" and adjacent use cases like ad targeting)
- [ ] Confirm walk-away minimums before any substantive meeting
