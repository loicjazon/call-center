/// Une seule vérification en vol à la fois : des lectures lancées en parallèle pourraient arriver dans le
/// désordre et faire réapparaître un statut périmé (« libre » alors que le collègue est en call).
/// Les demandes faites pendant une vérification sont regroupées en une seule relance.
#[derive(Default)]
pub struct PollGate {
    in_flight: bool,
    queued: bool,
}

impl PollGate {
    /// `true` si la vérification doit démarrer maintenant ; sinon elle est mise en attente.
    pub fn request(&mut self) -> bool {
        if self.in_flight {
            self.queued = true;
            false
        } else {
            self.in_flight = true;
            true
        }
    }

    /// `true` tant qu'une vérification est en cours (y compris une relance mise en attente).
    pub fn is_busy(&self) -> bool {
        self.in_flight
    }

    /// Démarre seulement si rien n'est en cours, sans mettre en attente (tour de minuterie : le suivant
    /// reviendra dans une minute).
    pub fn try_start(&mut self) -> bool {
        if self.in_flight {
            false
        } else {
            self.in_flight = true;
            true
        }
    }

    /// À appeler quand une vérification est terminée ; `true` si une relance en attente doit démarrer.
    pub fn finished(&mut self) -> bool {
        self.in_flight = false;
        if self.queued {
            self.queued = false;
            self.in_flight = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_request_starts_a_poll() {
        let mut gate = PollGate::default();

        assert!(gate.request());
    }

    #[test]
    fn request_while_a_poll_is_in_flight_does_not_start_another() {
        let mut gate = PollGate::default();
        gate.request();

        assert!(!gate.request());
    }

    #[test]
    fn queued_requests_coalesce_into_one_follow_up_poll() {
        let mut gate = PollGate::default();
        gate.request();
        gate.request();
        gate.request();

        assert!(gate.finished());
        // Le suivi est lui-même en vol : une seule relance, pas deux.
        assert!(!gate.finished());
    }

    #[test]
    fn finished_without_queued_request_allows_the_next_one() {
        let mut gate = PollGate::default();
        gate.request();

        assert!(!gate.finished());
        assert!(gate.request());
    }

    #[test]
    fn try_start_runs_when_idle() {
        let mut gate = PollGate::default();

        assert!(gate.try_start());
    }

    #[test]
    fn try_start_while_in_flight_is_skipped_and_not_queued() {
        let mut gate = PollGate::default();
        gate.request();

        assert!(!gate.try_start());
        assert!(!gate.finished());
    }

    #[test]
    fn gate_is_busy_from_the_start_of_a_poll_until_it_finishes() {
        let mut gate = PollGate::default();
        assert!(!gate.is_busy());

        gate.request();
        assert!(gate.is_busy());

        gate.finished();
        assert!(!gate.is_busy());
    }

    #[test]
    fn gate_stays_busy_when_a_queued_poll_restarts() {
        let mut gate = PollGate::default();
        gate.request();
        gate.request();

        assert!(gate.finished());
        assert!(gate.is_busy());
    }
}
