use serde::{Deserialize, Serialize};

pub mod hs_data;

pub use hs_data::get_database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HsCategory {
    pub hs_code: String,
    pub category: String,
    pub subcategory: String,
    pub description: String,
    pub tariff_rate: f64,
    pub vat_rate: f64,
    pub consumption_tax_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchCandidate {
    pub category: HsCategory,
    pub match_score: f64,
    pub match_level: String,
    pub matched_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRisk {
    pub risk_level: RiskLevel,
    pub risk_type: String,
    pub risk_code: String,
    pub description: String,
    pub suggestion: String,
    pub tax_difference_potential: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub primary: MatchCandidate,
    pub alternatives: Vec<MatchCandidate>,
    pub risks: Vec<TaxRisk>,
    pub is_ambiguous: bool,
    pub input_hs_code: String,
    pub normalized_hs_code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchItem {
    pub hs_code: String,
    pub item_name: Option<String>,
    pub transaction_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchConsistencyIssue {
    pub severity: RiskLevel,
    pub issue_type: String,
    pub description: String,
    pub affected_items: Vec<String>,
    pub suggestion: String,
    pub estimated_tax_variance: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchConsistencyResult {
    pub is_consistent: bool,
    pub issues: Vec<BatchConsistencyIssue>,
    pub total_items: usize,
    pub unique_hs_codes: usize,
    pub conflicting_categories: Vec<String>,
}

pub struct HsDatabase;

impl HsDatabase {
    pub fn lookup(hs_code: &str) -> Option<HsCategory> {
        Self::classify(hs_code).map(|r| r.primary.category)
    }

    pub fn classify(hs_code: &str) -> Option<ClassificationResult> {
        let cleaned = Self::normalize_hs_code(hs_code);
        if cleaned.is_empty() {
            return None;
        }

        let all_matches = Self::find_all_matches(&cleaned);
        if all_matches.is_empty() {
            return None;
        }

        let (primary, alternatives) = Self::select_best_matches(all_matches, &cleaned);
        let risks = Self::assess_risks(&primary, &alternatives, &cleaned);
        let is_ambiguous = alternatives.iter().any(|a| {
            (a.match_score - primary.match_score).abs() < 0.05
        });

        Some(ClassificationResult {
            primary,
            alternatives,
            risks,
            is_ambiguous,
            input_hs_code: hs_code.to_string(),
            normalized_hs_code: cleaned,
        })
    }

    pub fn check_batch_consistency(batch: &[BatchItem]) -> BatchConsistencyResult {
        let mut issues: Vec<BatchConsistencyIssue> = Vec::new();
        let mut conflicting_categories: Vec<String> = Vec::new();

        let unique_hs: std::collections::HashSet<String> = batch
            .iter()
            .map(|item| Self::normalize_hs_code(&item.hs_code))
            .collect();

        let mut category_groups: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut hs_to_rates: std::collections::HashMap<String, (f64, f64, f64)> =
            std::collections::HashMap::new();

        for item in batch {
            let normalized = Self::normalize_hs_code(&item.hs_code);
            if let Some(result) = Self::classify(&item.hs_code) {
                let cat_key = result.primary.category.category.clone();
                let item_id = item
                    .item_name
                    .clone()
                    .unwrap_or_else(|| format!("HS: {}", normalized));

                category_groups
                    .entry(cat_key.clone())
                    .or_default()
                    .push(item_id.clone());

                hs_to_rates.insert(
                    normalized.clone(),
                    (
                        result.primary.category.tariff_rate,
                        result.primary.category.vat_rate,
                        result.primary.category.consumption_tax_rate,
                    ),
                );

                if result.is_ambiguous {
                    let sample_price = item.transaction_price.unwrap_or(1000.0);
                    let mut variance = 0.0;
                    for alt in &result.alternatives {
                        if (alt.match_score - result.primary.match_score).abs() < 0.1 {
                            let primary_tax = sample_price
                                * (result.primary.category.tariff_rate
                                    + result.primary.category.vat_rate
                                    + result.primary.category.consumption_tax_rate);
                            let alt_tax = sample_price
                                * (alt.category.tariff_rate
                                    + alt.category.vat_rate
                                    + alt.category.consumption_tax_rate);
                            variance = variance.max((primary_tax - alt_tax).abs());
                        }
                    }

                    issues.push(BatchConsistencyIssue {
                        severity: if variance > sample_price * 0.05 {
                            RiskLevel::High
                        } else if variance > sample_price * 0.02 {
                            RiskLevel::Medium
                        } else {
                            RiskLevel::Low
                        },
                        issue_type: "AMBIGUOUS_CLASSIFICATION".to_string(),
                        description: format!(
                            "Item has ambiguous HS classification with multiple valid candidates",
                        ),
                        affected_items: vec![item
                            .item_name
                            .clone()
                            .unwrap_or_else(|| format!("HS: {}", normalized))],
                        suggestion: format!(
                            "Review HS code {} classification carefully, verify against product specifications and customs rulings",
                            normalized
                        ),
                        estimated_tax_variance: if variance > 0.0 { Some(variance) } else { None },
                    });
                }
            }
        }

        let rates: Vec<&(f64, f64, f64)> = hs_to_rates.values().collect();
        if rates.len() >= 2 {
            let mut max_tariff = 0.0;
            let mut min_tariff = f64::MAX;
            let mut max_consumption = 0.0;
            for (t, _, c) in &rates {
                max_tariff = max_tariff.max(*t);
                min_tariff = min_tariff.min(*t);
                max_consumption = max_consumption.max(*c);
            }
            if (max_tariff - min_tariff) > 0.10 {
                conflicting_categories.push("Significant tariff rate variance in batch".to_string());
                issues.push(BatchConsistencyIssue {
                    severity: RiskLevel::High,
                    issue_type: "RATE_VARIANCE".to_string(),
                    description: format!(
                        "Tariff rates vary significantly across batch from {:.1}% to {:.1}%",
                        min_tariff * 100.0, max_tariff * 100.0
                    ),
                    affected_items: batch
                        .iter()
                        .map(|i| {
                            i.item_name
                                .clone()
                                .unwrap_or_else(|| format!("HS: {}", Self::normalize_hs_code(&i.hs_code)))
                        })
                        .collect(),
                    suggestion: "Verify that all items are correctly categorized; rate variance may indicate misclassification or mixed product types".to_string(),
                    estimated_tax_variance: None,
                });
            }
            if max_consumption > 0.0 {
                let has_non_taxed = rates.iter().any(|(_, _, c)| *c == 0.0);
                if has_non_taxed {
                    conflicting_categories.push("Mixed consumption tax treatment".to_string());
                    issues.push(BatchConsistencyIssue {
                        severity: RiskLevel::Medium,
                        issue_type: "MIXED_CONSUMPTION_TAX".to_string(),
                        description:
                            "Some items are subject to consumption tax while others are not in the same batch"
                                .to_string(),
                        affected_items: batch
                            .iter()
                            .map(|i| {
                                i.item_name
                                    .clone()
                                    .unwrap_or_else(|| format!("HS: {}", Self::normalize_hs_code(&i.hs_code)))
                            })
                            .collect(),
                        suggestion: "Verify products are not misclassified; consumption tax differences may indicate mixed product categories".to_string(),
                        estimated_tax_variance: None,
                    });
                }
            }
        }

        if category_groups.len() > 1 {
            let large_groups: Vec<&String> = category_groups
                .iter()
                .filter(|(_, v)| v.len() > 1)
                .map(|(k, _)| k)
                .collect();
            if large_groups.len() > 1 {
                conflicting_categories.extend(large_groups.into_iter().cloned());
                issues.push(BatchConsistencyIssue {
                    severity: RiskLevel::Medium,
                    issue_type: "MIXED_CATEGORIES".to_string(),
                    description: format!(
                        "Batch contains items across {} different major categories",
                        category_groups.len()
                    ),
                    affected_items: batch
                        .iter()
                        .map(|i| {
                            i.item_name
                                .clone()
                                .unwrap_or_else(|| format!("HS: {}", Self::normalize_hs_code(&i.hs_code)))
                        })
                        .collect(),
                    suggestion: "Ensure batch contains related products; mixed categories may cause customs scrutiny".to_string(),
                    estimated_tax_variance: None,
                });
            }
        }

        let is_consistent = issues.is_empty()
            || issues
                .iter()
                .all(|i| i.severity == RiskLevel::Low);

        BatchConsistencyResult {
            is_consistent,
            issues,
            total_items: batch.len(),
            unique_hs_codes: unique_hs.len(),
            conflicting_categories,
        }
    }

    pub fn list_categories() -> Vec<HsCategory> {
        get_database()
    }

    fn normalize_hs_code(hs_code: &str) -> String {
        let mut cleaned: String = hs_code
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        if cleaned.len() > 10 {
            cleaned.truncate(10);
        }
        cleaned
    }

    fn find_all_matches(cleaned: &str) -> Vec<MatchCandidate> {
        let db = get_database();
        let mut matches: Vec<MatchCandidate> = Vec::new();

        for cat in db {
            if let Some(score) = Self::calculate_match_score(cleaned, &cat.hs_code) {
                let match_level = if cat.hs_code.len() == cleaned.len() && cat.hs_code == cleaned {
                    "Exact".to_string()
                } else if cleaned.len() >= 6 && cat.hs_code.len() >= 6
                    && cleaned[..6] == cat.hs_code[..6]
                {
                    "6-digit".to_string()
                } else if cleaned.len() >= 4 && cat.hs_code.len() >= 4
                    && cleaned[..4] == cat.hs_code[..4]
                {
                    "4-digit".to_string()
                } else {
                    "Chapter".to_string()
                };

                let common_prefix = Self::common_prefix_length(cleaned, &cat.hs_code);
                let matched_prefix = cleaned[..common_prefix].to_string();

                matches.push(MatchCandidate {
                    category: cat,
                    match_score: score,
                    match_level,
                    matched_prefix,
                });
            }
        }

        matches.sort_by(|a, b| {
            b.match_score
                .partial_cmp(&a.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        matches
    }

    fn calculate_match_score(input: &str, code: &str) -> Option<f64> {
        let common = Self::common_prefix_length(input, code);
        if common == 0 {
            return None;
        }

        let input_len = input.len() as f64;
        let code_len = code.len() as f64;
        let common_len = common as f64;

        let prefix_score = common_len / input_len.max(code_len);
        let exact_match = if input == code { 1.0 } else { 0.0 };
        let length_bonus = if common >= 6 {
            0.2
        } else if common >= 4 {
            0.1
        } else {
            0.0
        };
        let specificity_bonus = if code.len() > input.len() {
            0.0
        } else {
            (code_len / 10.0) * 0.15
        };

        let total = (prefix_score * 0.6) + (exact_match * 0.3) + length_bonus + specificity_bonus;

        Some(total.min(1.0))
    }

    fn common_prefix_length(a: &str, b: &str) -> usize {
        a.chars()
            .zip(b.chars())
            .take_while(|(x, y)| x == y)
            .count()
    }

    fn select_best_matches(
        mut all_matches: Vec<MatchCandidate>,
        _input: &str,
    ) -> (MatchCandidate, Vec<MatchCandidate>) {
        all_matches.sort_by(|a, b| {
            b.match_score
                .partial_cmp(&a.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let primary = all_matches.remove(0);
        let alternatives: Vec<MatchCandidate> = all_matches
            .into_iter()
            .filter(|m| m.match_score >= 0.3)
            .take(3)
            .collect();

        (primary, alternatives)
    }

    fn assess_risks(
        primary: &MatchCandidate,
        alternatives: &[MatchCandidate],
        input: &str,
    ) -> Vec<TaxRisk> {
        let mut risks: Vec<TaxRisk> = Vec::new();

        if input.len() < 4 {
            risks.push(TaxRisk {
                risk_level: RiskLevel::High,
                risk_type: "INSUFFICIENT_PRECISION".to_string(),
                risk_code: "R001".to_string(),
                description: format!(
                    "HS code {} has insufficient digits ({}). 4+ digits required for reliable classification",
                    input, input.len()
                ),
                suggestion: "Provide at least 4-digit HS code; 6-10 digits recommended for accurate tariff determination".to_string(),
                tax_difference_potential: None,
            });
        } else if input.len() < 6 {
            risks.push(TaxRisk {
                risk_level: RiskLevel::Medium,
                risk_type: "LOW_PRECISION".to_string(),
                risk_code: "R002".to_string(),
                description: format!(
                    "HS code {} has only {} digits. Using chapter-level classification may result in incorrect rate application",
                    input, input.len()
                ),
                suggestion: "Provide 6+ digit HS code for subheading-level accuracy".to_string(),
                tax_difference_potential: None,
            });
        }

        if primary.match_level != "Exact" && input.len() >= 4 {
            risks.push(TaxRisk {
                risk_level: RiskLevel::Medium,
                risk_type: "NON_EXACT_MATCH".to_string(),
                risk_code: "R003".to_string(),
                description: format!(
                    "No exact match found for HS code {}. Matched at {} level with code {}",
                    input, primary.match_level, primary.category.hs_code
                ),
                suggestion: "Verify product specifications against customs tariff database to confirm correct classification".to_string(),
                tax_difference_potential: None,
            });
        }

        if primary.category.consumption_tax_rate > 0.0 {
            risks.push(TaxRisk {
                risk_level: RiskLevel::Medium,
                risk_type: "CONSUMPTION_TAX_APPLICABLE".to_string(),
                risk_code: "R004".to_string(),
                description: format!(
                    "Product category is subject to {:.1}% consumption tax, verify if any exemption applies",
                    primary.category.consumption_tax_rate * 100.0
                ),
                suggestion: "Check if product qualifies for consumption tax exemption or reduced rate based on end use and customs regulations".to_string(),
                tax_difference_potential: None,
            });
        }

        let high_rate_categories = [
            "Passenger Vehicles", "Tobacco Products", "Distilled Spirits",
            "Jewelry", "Cosmetics", "Clocks and Watches",
        ];
        if high_rate_categories.contains(&primary.category.subcategory.as_str())
            || high_rate_categories.contains(&primary.category.category.as_str())
        {
            risks.push(TaxRisk {
                risk_level: RiskLevel::Medium,
                risk_type: "HIGH_RATE_CATEGORY".to_string(),
                risk_code: "R005".to_string(),
                description: format!(
                    "Product falls under high-duty category ({}), subject to enhanced customs scrutiny",
                    primary.category.subcategory
                ),
                suggestion: "Ensure complete product documentation and value declaration are prepared; consider obtaining advance customs ruling for high-value shipments".to_string(),
                tax_difference_potential: None,
            });
        }

        for alt in alternatives {
            let rate_diff = (primary.category.tariff_rate - alt.category.tariff_rate).abs()
                + (primary.category.consumption_tax_rate - alt.category.consumption_tax_rate).abs();

            if (alt.match_score - primary.match_score).abs() < 0.08 && rate_diff > 0.03 {
                risks.push(TaxRisk {
                    risk_level: RiskLevel::High,
                    risk_type: "CLASSIFICATION_AMBIGUITY".to_string(),
                    risk_code: "R006".to_string(),
                    description: format!(
                        "Multiple plausible classifications with significant rate difference. Primary: {} ({:.1}% tariff, {:.1}% consumption), Alternative: {} ({:.1}% tariff, {:.1}% consumption)",
                        primary.category.subcategory,
                        primary.category.tariff_rate * 100.0,
                        primary.category.consumption_tax_rate * 100.0,
                        alt.category.subcategory,
                        alt.category.tariff_rate * 100.0,
                        alt.category.consumption_tax_rate * 100.0,
                    ),
                    suggestion: format!(
                        "Critical: Obtain official customs classification ruling or consult customs broker. Incorrect classification between these categories could result in tax underpayment penalties up to {:.1}% of CIF value",
                        rate_diff * 100.0
                    ),
                    tax_difference_potential: Some(rate_diff),
                });
            } else if (alt.match_score - primary.match_score).abs() < 0.12
                && rate_diff > 0.01
            {
                risks.push(TaxRisk {
                    risk_level: RiskLevel::Medium,
                    risk_type: "CLASSIFICATION_UNCERTAINTY".to_string(),
                    risk_code: "R007".to_string(),
                    description: format!(
                        "Classification uncertainty: alternative category {} ({}) has similar match score with rate difference of {:.1}%",
                        alt.category.subcategory,
                        alt.category.hs_code,
                        rate_diff * 100.0,
                    ),
                    suggestion: "Review product specifications carefully against tariff heading notes; consider requesting a binding tariff information (BTI) ruling".to_string(),
                    tax_difference_potential: Some(rate_diff),
                });
            }
        }

        if primary.category.tariff_rate >= 0.20 {
            risks.push(TaxRisk {
                risk_level: RiskLevel::Medium,
                risk_type: "HIGH_TARIFF".to_string(),
                risk_code: "R008".to_string(),
                description: format!(
                    "Applicable tariff rate is {:.1}%, verify if preferential tariff treatment is available under applicable FTAs",
                    primary.category.tariff_rate * 100.0
                ),
                suggestion: "Check eligibility for preferential rates under free trade agreements (e.g., RCEP, CAI). Proper certificate of origin may significantly reduce duty burden.".to_string(),
                tax_difference_potential: Some(primary.category.tariff_rate * 0.5),
            });
        }

        risks.sort_by(|a, b| {
            let order_a = match a.risk_level {
                RiskLevel::Critical => 0,
                RiskLevel::High => 1,
                RiskLevel::Medium => 2,
                RiskLevel::Low => 3,
            };
            let order_b = match b.risk_level {
                RiskLevel::Critical => 0,
                RiskLevel::High => 1,
                RiskLevel::Medium => 2,
                RiskLevel::Low => 3,
            };
            order_a.cmp(&order_b)
        });

        risks
    }
}
