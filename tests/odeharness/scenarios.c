/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The comparison's scenarios (harness.h): synthetic scenes of our own, no
** game data, driven through the library's public API in a fixed order, and
** what the library computes recorded after every step. Each starts with the
** shipped library's floating-point environment (24-bit precision, round to
** nearest, exceptions masked; MXCSR flush-to-zero and denormals-are-zero)
** and the run time's rand at 1, except the *-nodaz ones (MXCSR's default),
** which tell whether denormals change the shipped library's results (#769).
**
** Inputs are made from integers and bit patterns; the only float arithmetic
** here is exact or single precision on both sides (SSE on Windows).
*/

#include "harness.h"

#include <string.h>

#define NONE ((void *) 0)

static const OdeHarness *H;

/* ---- output ---- */

enum {
  T_PARAMS = 1, T_X87, T_VEC, T_BODY, T_FEEDBACK, T_JOINT, T_CONTACTS,
  T_COUNT, T_GEOM, T_FLAG
};

static uint32_t bits (float f)
{
  uint32_t u;
  memcpy (&u, &f, 4);
  return u;
}

static float from_bits (uint32_t u)
{
  float f;
  memcpy (&f, &u, 4);
  return f;
}

static void rec (uint32_t tag, const uint32_t *w, uint32_t n)
{
  H->record (H->ctx, tag, w, n);
}

static void rec_floats (uint32_t tag, const dReal *v, uint32_t n)
{
  uint32_t w[64];
  for (uint32_t i = 0; i < n; i++) w[i] = bits (v[i]);
  rec (tag, w, n);
}

static void rec_int (uint32_t tag, uint32_t v)
{
  rec (tag, &v, 1);
}

static void rec_body (dBodyID b)
{
  dReal v[25];
  memcpy (v, H->dBodyGetPosition (b), 3 * 4);
  memcpy (v + 3, H->dBodyGetQuaternion (b), 4 * 4);
  memcpy (v + 7, H->dBodyGetRotation (b), 12 * 4);
  memcpy (v + 19, H->dBodyGetLinearVel (b), 3 * 4);
  memcpy (v + 22, H->dBodyGetAngularVel (b), 3 * 4);
  rec_floats (T_BODY, v, 25);
}

static void rec_params (dWorldID w)
{
  dReal v[12];
  H->dWorldGetGravity (w, v);
  v[3] = H->dWorldGetERP (w);
  v[4] = H->dWorldGetCFM (w);
  v[5] = from_bits ((uint32_t) H->dWorldGetQuickStepNumIterations (w));
  v[6] = H->dWorldGetQuickStepW (w);
  v[7] = H->dWorldGetContactSurfaceLayer (w);
  v[8] = H->dWorldGetContactMaxCorrectingVel (w);
  v[9] = H->dWorldGetAutoDisableLinearThreshold (w);
  v[10] = from_bits ((uint32_t) H->dWorldGetAutoDisableSteps (w));
  v[11] = from_bits ((uint32_t) H->dWorldGetAutoDisableFlag (w));
  rec_floats (T_PARAMS, v, 12);
}

/* ---- inputs: integers and bit patterns ---- */

static uint32_t seed;

static uint32_t next (void)
{
  seed = seed * 1664525u + 1013904223u;
  return seed >> 8;
}

/* a float in [lo, hi): an integer of 24 bits scaled exactly, then one
** single-precision multiply-add (the same on both sides) */
static float uni (float lo, float hi)
{
  float u = (float) next () * (1.0f / 16777216.0f);
  return lo + (hi - lo) * u;
}

/* a float with random sign and mantissa, its exponent in [emin, emax] */
static float spread (int emin, int emax)
{
  uint32_t e = (uint32_t) (emin + 127) + next () % (uint32_t) (emax - emin + 1);
  return from_bits ((next () & 1u) << 31 | e << 23 | (next () & 0x7FFFFFu));
}

static void rand_rotation (dReal *R)
{
  float ax = uni (-1, 1), ay = uni (-1, 1), az = uni (-1, 1);
  H->dRFromAxisAndAngle (R, ax, ay, az + 0.01f, uni (-3.1f, 3.1f));
}

/* ---- the game's step and parameters (physics-world.md 1.2, 1.3) ---- */

static const float STEP = 0.02f;

static dWorldID game_world (void)
{
  dWorldID w = H->dWorldCreate ();
  H->dWorldSetGravity (w, 0, -19.62f, 0);
  H->dWorldSetERP (w, 0.54545456f);	/* 6/11 */
  H->dWorldSetCFM (w, 1.1363636e-6f);	/* 1/880,000 */
  return w;
}

static void mass_box (dBodyID b, float total, float lx, float ly, float lz)
{
  dMass m;
  H->dMassSetBoxTotal (&m, total, lx, ly, lz);
  H->dBodySetMass (b, &m);
}

/* ---- scenarios ---- */

/* the library's values when it loads, then after dWorldCreate */
static void s_library (void)
{
  /* a stand-in world: the getters read the library-wide values */
  static uint32_t stand_in[16];
  rec_params ((dWorldID) (void *) stand_in);
  dWorldID w = H->dWorldCreate ();
  rec_params (w);
  H->dWorldDestroy (w);
}

/* the x87 instructions themselves, against our emulation */
static void s_x87 (void)
{
  static const uint32_t special[] = {
    0x00000000, 0x80000000, 0x00000001, 0x007FFFFF, 0x00800000, 0x3F800000,
    0x40490FDB, 0xC0490FDB, 0x3FC90FDB, 0x3F490FDB, 0x40C90FDB, 0x41490FDB,
    0x4B800000, 0x5F000000, 0x5EFFFFFF, 0x7F7FFFFF, 0x7F800000, 0xFF800000,
    0x7FC00000, 0x2F800000, 0x38D1B717, 0x3EAAAAAB, 0x447A0000, 0x49742400
  };
  int n = (int) (sizeof special / sizeof special[0]);
  for (int op = 0; op < 3; op++) {
    uint32_t w[2];
    for (int i = 0; i < n; i++) {
      w[0] = special[i];
      w[1] = bits (H->x87 (op, 0, from_bits (special[i])));
      rec (T_X87 + (uint32_t) (op << 8), w, 2);
    }
    for (int i = 0; i < 4000; i++) {
      float x = op == 0 ? from_bits (bits (spread (-140, 127)) & 0x7FFFFFFF)
		: (i & 1) ? spread (-12, 4) : uni (-30.0f, 30.0f);
      w[0] = bits (x);
      w[1] = bits (H->x87 (op, 0, x));
      rec (T_X87 + (uint32_t) (op << 8), w, 2);
    }
  }
  for (int i = 0; i < n * n; i += 7) {
    uint32_t w[3] = {special[i / n], special[i % n], 0};
    w[2] = bits (H->x87 (3, from_bits (w[0]), from_bits (w[1])));
    rec (T_X87 + (3 << 8), w, 3);
  }
  for (int i = 0; i < 4000; i++) {
    float y = (i & 1) ? spread (-30, 30) : uni (-5, 5);
    float x = (i & 2) ? spread (-30, 30) : uni (-5, 5);
    uint32_t w[3] = {bits (y), bits (x), bits (H->x87 (3, y, x))};
    rec (T_X87 + (3 << 8), w, 3);
  }
}

static void s_normalise (void)
{
  for (int i = 0; i < 2000; i++) {
    int e = (int) (next () % 40) - 30;
    dReal v[4] = {spread (e - 3, e), spread (e - 3, e), spread (e - 3, e), 0};
    if (i % 50 == 0) v[0] = v[1] = v[2] = 0;
    dReal in[3] = {v[0], v[1], v[2]};
    H->dNormalize3_slow (v);
    rec_floats (T_VEC, in, 3);
    rec_floats (T_VEC, v, 3);
  }
}

/* bodies without joints or contacts: forces, torques, the gyroscopic term,
** finite rotations, no gravity; both steppers */
static void s_free (void)
{
  dWorldID w = game_world ();
  dBodyID b[8];
  for (int i = 0; i < 8; i++) {
    b[i] = H->dBodyCreate (w);
    mass_box (b[i], uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    H->dBodySetPosition (b[i], uni (-5, 5), uni (0, 10), uni (-5, 5));
    dReal R[12];
    rand_rotation (R);
    H->dBodySetRotation (b[i], R);
    H->dBodySetLinearVel (b[i], uni (-3, 3), uni (-3, 3), uni (-3, 3));
    H->dBodySetAngularVel (b[i], uni (-5, 5), uni (-5, 5), uni (-5, 5));
  }
  H->dBodySetFiniteRotationMode (b[1], 1);
  H->dBodySetFiniteRotationMode (b[2], 1);
  H->dBodySetFiniteRotationAxis (b[2], 0.3f, 1, 0.2f);
  H->dBodySetGravityMode (b[3], 0);
  for (int s = 0; s < 200; s++) {
    for (int i = 0; i < 8; i++) {
      H->dBodyAddForce (b[i], uni (-50, 50), uni (-50, 50), uni (-50, 50));
      H->dBodyAddTorque (b[i], uni (-1, 1), uni (-1, 1), uni (-1, 1));
      H->dBodyAddRelForceAtRelPos (b[i], uni (-9, 9), 0, uni (-9, 9), 0.1f, 0.2f, uni (-1, 1));
    }
    if (s < 100) H->dWorldQuickStep (w, STEP);
    else H->dWorldStep (w, STEP);
    for (int i = 0; i < 8; i++) rec_body (b[i]);
  }
  H->dWorldDestroy (w);
}

/* every joint kind, with limits, motors and feedback; both steppers */
static void s_joints (void)
{
  dWorldID w = game_world ();
  enum { N = 9 };
  dBodyID b[N];
  for (int i = 0; i < N; i++) {
    b[i] = H->dBodyCreate (w);
    mass_box (b[i], uni (2, 40), 0.3f, 0.3f, 0.6f);
    H->dBodySetPosition (b[i], (float) i * 0.5f, 5, 0);
    H->dBodySetAngularVel (b[i], uni (-2, 2), uni (-2, 2), uni (-2, 2));
  }
  dJointID j[N];
  static dJointFeedback fb[N];
  j[0] = H->dJointCreateBall (w, 0);
  H->dJointAttach (j[0], b[0], NONE);		/* hung from the world */
  H->dJointSetBallAnchor (j[0], -0.25f, 5, 0);
  j[1] = H->dJointCreateHinge (w, 0);
  H->dJointAttach (j[1], b[0], b[1]);
  H->dJointSetHingeAnchor (j[1], 0.25f, 5, 0);
  H->dJointSetHingeAxis (j[1], 0, 0, 1);
  H->dJointSetHingeParam (j[1], dParamLoStop, -0.5f);
  H->dJointSetHingeParam (j[1], dParamHiStop, 0.7f);
  H->dJointSetHingeParam (j[1], dParamBounce, 0.3f);
  j[2] = H->dJointCreateHinge (0, 0);		/* made without a world */
  H->dWorldAddJoint (w, j[2]);
  H->dJointAttach (j[2], b[1], b[2]);
  H->dJointSetHingeAnchor (j[2], 0.75f, 5, 0);
  H->dJointSetHingeAxis (j[2], 0, 1, 0);
  H->dJointSetHingeParam (j[2], dParamVel, 2);
  H->dJointSetHingeParam (j[2], dParamFMax, 30);
  j[3] = H->dJointCreateSlider (w, 0);
  H->dJointAttach (j[3], b[2], b[3]);
  H->dJointSetSliderAxis (j[3], 1, 0.2f, 0);
  H->dJointSetSliderParam (j[3], dParamLoStop, -0.1f);
  H->dJointSetSliderParam (j[3], dParamHiStop, 0.2f);
  j[4] = H->dJointCreateUniversal (w, 0);
  H->dJointAttach (j[4], b[3], b[4]);
  H->dJointSetUniversalAnchor (j[4], 1.75f, 5, 0);
  H->dJointSetUniversalAxis1 (j[4], 0, 1, 0);
  H->dJointSetUniversalAxis2 (j[4], 0, 0, 1);
  j[5] = H->dJointCreateHinge2 (w, 0);
  H->dJointAttach (j[5], b[4], b[5]);
  H->dJointSetHinge2Anchor (j[5], 2.25f, 5, 0);
  H->dJointSetHinge2Axis1 (j[5], 0, 1, 0);
  H->dJointSetHinge2Axis2 (j[5], 1, 0, 0);
  H->dJointSetHinge2Param (j[5], dParamSuspensionERP, 0.4f);
  H->dJointSetHinge2Param (j[5], dParamSuspensionCFM, 0.01f);
  j[6] = H->dJointCreateFixed (w, 0);
  H->dJointAttach (j[6], b[5], b[6]);
  H->dJointSetFixed (j[6]);
  j[7] = H->dJointCreateAMotor (w, 0);
  H->dJointAttach (j[7], b[6], b[7]);
  H->dJointSetAMotorNumAxes (j[7], 1);
  H->dJointSetAMotorAxis (j[7], 0, 1, 0, 0, 1);
  H->dJointSetAMotorParam (j[7], dParamVel, -1);
  H->dJointSetAMotorParam (j[7], dParamFMax, 10);
  {
    /* the motor turns b7; a ball holds it */
    dJointID ball = H->dJointCreateBall (w, 0);
    H->dJointAttach (ball, b[6], b[7]);
    H->dJointSetBallAnchor (ball, 3.25f, 5, 0);
  }
  j[8] = H->dJointCreateFixed (w, 0);		/* one body held at a pose */
  H->dJointAttach (j[8], b[8], NONE);
  {
    dReal q[4] = {0.9238795f, 0, 0.38268343f, 0}, p[4] = {4, 6, 1, 0};
    H->dJointSetFixedQuaternionPos (j[8], q, p);
  }
  for (int i = 0; i < N; i++) H->dJointSetFeedback (j[i], &fb[i]);
  for (int s = 0; s < 300; s++) {
    H->dBodyAddForce (b[N / 2], uni (-20, 20), uni (-20, 20), uni (-20, 20));
    if (s < 150) H->dWorldQuickStep (w, STEP);
    else H->dWorldStep (w, STEP);
    for (int i = 0; i < N; i++) rec_body (b[i]);
    for (int i = 0; i < N; i++) rec_floats (T_FEEDBACK, (const dReal *) &fb[i], 16);
    dReal a[4] = {H->dJointGetHingeAngle (j[1]), H->dJointGetHingeAngleRate (j[1]),
		  H->dJointGetHingeAngle (j[2]), H->dJointGetSliderPosition (j[3])};
    rec_floats (T_JOINT, a, 4);
  }
  H->dWorldDestroy (w);
}

/* a pile on a plane: contacts of both kinds through dCollide each step */
typedef struct Pile {
  dWorldID w;
  dSpaceID space;
  dJointGroupID group;
  dJointID made[512];
  int nmade, step_contacts;
} Pile;

static void near_cb (void *data, dGeomID a, dGeomID b)
{
  Pile *p = (Pile *) data;
  dBodyID ba = H->dGeomGetBody (a), bb = H->dGeomGetBody (b);
  if (ba && bb && H->dAreConnected (ba, bb)) return;
  dContactGeom cg[16];
  int n = H->dCollide (a, b, 8, cg, (int) sizeof (dContactGeom));
  uint32_t w[1 + 8 * 7];
  w[0] = (uint32_t) n;
  for (int i = 0; i < n && i < 8; i++) {
    for (int k = 0; k < 3; k++) w[1 + i * 7 + k] = bits (cg[i].pos[k]);
    for (int k = 0; k < 3; k++) w[4 + i * 7 + k] = bits (cg[i].normal[k]);
    w[7 + i * 7] = bits (cg[i].depth);
  }
  rec (T_CONTACTS, w, 1 + (uint32_t) (n < 8 ? n : 8) * 7);
  for (int i = 0; i < n && p->nmade < 512; i++) {
    dContact c;
    memset (&c, 0, sizeof c);
    int kind = (p->step_contacts + i) % 4;
    c.surface.mode = dContactApprox1 | dContactSoftERP | dContactSoftCFM;
    c.surface.mu = kind == 3 ? 0.3f : 1.0f;
    c.surface.soft_erp = 0.5f;
    c.surface.soft_cfm = 1e-5f;
    if (kind == 1) {
      c.surface.mode |= dContactBounce;
      c.surface.bounce = 0.4f;
      c.surface.bounce_vel = 0.1f;
    }
    if (kind == 2) {
      c.surface.mode |= dContactMu2;
      c.surface.mu2 = 0.5f;
    }
    c.geom = cg[i];
    dJointID j = (i & 1) ? H->dJointCreateContactSpecial (0, p->group, &c)
			 : H->dJointCreateContact (0, p->group, &c);
    H->dWorldAddJoint (p->w, j);
    H->dJointAttach (j, ba, bb);
    p->made[p->nmade++] = j;
  }
  p->step_contacts += n;
}

static void pile_step (Pile *p, int exact)
{
  p->nmade = 0;
  p->step_contacts = 0;
  H->dSpaceCollide (p->space, p, near_cb);
  rec_int (T_COUNT, (uint32_t) p->nmade);
  if (exact) H->dWorldStep (p->w, STEP);
  else H->dWorldQuickStep (p->w, STEP);
  /* the contacts detached and out of the world, then the group emptied
  ** (emptying detaches only joints still in a world, and leaves them
  ** listed: physics-library.md 5.2) */
  for (int i = 0; i < p->nmade; i++) {
    H->dJointAttach (p->made[i], NONE, NONE);
    H->dWorldRemoveJoint (p->w, p->made[i]);
  }
  H->dJointGroupEmpty (p->group);
}

static void pile (int count, int steps, int exact, float heavy)
{
  Pile p;
  p.w = game_world ();
  p.space = H->dSimpleSpaceCreate (0);
  p.group = H->dJointGroupCreate (0);
  H->dCreatePlane (p.space, 0, 1, 0, 0);
  dBodyID b[32];
  for (int i = 0; i < count; i++) {
    b[i] = H->dBodyCreate (p.w);
    dGeomID g;
    float s1 = uni (0.2f, 1), s2 = uni (0.2f, 1), s3 = uni (0.2f, 1);
    float m = heavy > 0 ? heavy : uni (5, 100);
    dMass ms;
    switch (i % 3) {
    case 0:
      H->dMassSetBoxTotal (&ms, m, s1, s2, s3);
      g = H->dCreateBox (p.space, s1, s2, s3);
      break;
    case 1:
      H->dMassSetSphereTotal (&ms, m, s1 * 0.5f);
      g = H->dCreateSphere (p.space, s1 * 0.5f);
      break;
    default:
      H->dMassSetCappedCylinderTotal (&ms, m, 3, s1 * 0.3f, s2);
      g = H->dCreateCCylinder (p.space, s1 * 0.3f, s2);
      break;
    }
    H->dBodySetMass (b[i], &ms);
    H->dGeomSetBody (g, b[i]);
    H->dBodySetPosition (b[i], uni (-1, 1), 0.6f + (float) i * 0.8f, uni (-1, 1));
    dReal R[12];
    rand_rotation (R);
    H->dBodySetRotation (b[i], R);
  }
  for (int s = 0; s < steps; s++) {
    pile_step (&p, exact);
    for (int i = 0; i < count; i++) rec_body (b[i]);
  }
  H->dJointGroupDestroy (p.group);
  H->dSpaceDestroy (p.space);
  H->dWorldDestroy (p.w);
}

static void s_contacts (void) { pile (12, 400, 0, 0); }
static void s_contacts_exact (void) { pile (6, 200, 1, 0); }
static void s_heavy (void) { pile (6, 150, 0, 20000); }

/* geometry pairs through dCollide, at most 1 and at most 8 asked for */
static void s_pairs (void)
{
  dGeomID box1 = H->dCreateBox (0, 1, 1, 1), box2 = H->dCreateBox (0, 1, 1, 1);
  dGeomID sphere = H->dCreateSphere (0, 0.5f), ccyl = H->dCreateCCylinder (0, 0.3f, 1);
  dGeomID plane = H->dCreatePlane (0, 0, 1, 0, 0.1f);
  dGeomID first[5] = {box1, sphere, ccyl, ccyl, box1};
  dGeomID second[5] = {box2, box2, box2, ccyl, plane};
  for (int k = 0; k < 5; k++) {
    for (int i = 0; i < 1500; i++) {
      dReal R[12];
      H->dGeomSetPosition (first[k], 0, 0, 0);
      rand_rotation (R);
      H->dGeomSetRotation (first[k], R);
      if (second[k] != plane && second[k] != first[k]) {
	rand_rotation (R);
	H->dGeomSetRotation (second[k], R);
	H->dGeomSetPosition (second[k], uni (-1.2f, 1.2f), uni (-1.2f, 1.2f), uni (-1.2f, 1.2f));
      }
      if (k == 3) {
	/* two capped cylinders: a second one */
	static dGeomID other;
	if (!other) other = H->dCreateCCylinder (0, 0.25f, 0.8f);
	rand_rotation (R);
	H->dGeomSetRotation (other, R);
	H->dGeomSetPosition (other, uni (-1, 1), uni (-1, 1), uni (-1, 1));
	second[k] = other;
      }
      for (int max = 1; max <= 8; max += 7) {
	dContactGeom cg[16];
	int n = H->dCollide (first[k], second[k], max, cg, (int) sizeof (dContactGeom));
	uint32_t w[2 + 16 * 7];
	w[0] = (uint32_t) k;
	w[1] = (uint32_t) n;
	for (int c = 0; c < n && c < 16; c++) {
	  for (int e = 0; e < 3; e++) w[2 + c * 7 + e] = bits (cg[c].pos[e]);
	  for (int e = 0; e < 3; e++) w[5 + c * 7 + e] = bits (cg[c].normal[e]);
	  w[8 + c * 7] = bits (cg[c].depth);
	}
	rec (T_CONTACTS, w, 2 + (uint32_t) (n < 16 ? n : 16) * 7);
      }
    }
  }
  H->dGeomDestroy (box1);
  H->dGeomDestroy (box2);
  H->dGeomDestroy (sphere);
  H->dGeomDestroy (ccyl);
  H->dGeomDestroy (plane);
}

/* rays set from a start and direction, against every class */
static void s_rays (void)
{
  dGeomID ray = H->dCreateRay (0, 5);
  dGeomID target[3] = {H->dCreateSphere (0, 0.7f), H->dCreateBox (0, 1, 0.5f, 2),
		       H->dCreateCCylinder (0, 0.4f, 1.5f)};
  int (*direct[3]) (dGeomID, dGeomID, int, dContactGeom *, int) = {
    H->dCollideRaySphere, H->dCollideRayBox, H->dCollideRayCCylinder};
  for (int i = 0; i < 3000; i++) {
    H->dGeomRaySet (ray, uni (-3, 3), uni (-3, 3), uni (-3, 3), uni (-1, 1), uni (-1, 1), uni (-1, 1));
    dReal g[15];
    memcpy (g, H->dGeomGetPosition (ray), 12);
    memcpy (g + 3, H->dGeomGetRotation (ray), 48);
    rec_floats (T_GEOM, g, 15);
    int t = i % 3;
    dReal R[12];
    rand_rotation (R);
    H->dGeomSetRotation (target[t], R);
    dContactGeom c[2];
    int n = (i & 1) ? H->dCollide (ray, target[t], 1, c, (int) sizeof c[0])
		    : direct[t] (ray, target[t], 1, c, (int) sizeof c[0]);
    dReal v[8] = {from_bits ((uint32_t) n), c[0].pos[0], c[0].pos[1], c[0].pos[2],
		  c[0].normal[0], c[0].normal[1], c[0].normal[2], c[0].depth};
    rec_floats (T_GEOM, v, n ? 8 : 1);
  }
  /* a ray on a body moves and turns the body */
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  H->dGeomSetBody (ray, b);
  H->dGeomRaySet (ray, 1, 2, 3, 0.2f, -1, 0.4f);
  rec_body (b);
  H->dGeomDestroy (ray);
  for (int t = 0; t < 3; t++) H->dGeomDestroy (target[t]);
  H->dWorldDestroy (w);
}

/* worlds as lists: bodies and joints outside a world, added and removed,
** disabled bodies, parameters shared by worlds, emptied groups, dxStepBody */
static void s_worlds (void)
{
  dWorldID a = game_world (), c = H->dWorldCreate ();
  rec_params (a);			/* c's creation reset them */
  H->dWorldSetGravity (c, 0, -19.62f, 0);
  dBodyID b[4];
  for (int i = 0; i < 4; i++) {
    b[i] = H->dBodyCreate (i < 2 ? a : NONE);
    mass_box (b[i], 10, 1, 1, 1);
    H->dBodySetPosition (b[i], (float) i * 3, 1, 0);
    H->dBodySetAngularVel (b[i], 1, 2, 3);
  }
  H->dBodyDisable (b[1]);
  H->dWorldAddBody (a, b[2]);
  dJointGroupID g = H->dJointGroupCreate (0);
  dJointID j = H->dJointCreateBall (0, g);
  H->dWorldAddJoint (a, j);
  H->dJointAttach (j, b[0], b[2]);
  H->dJointSetBallAnchor (j, 1.5f, 1, 0);
  for (int s = 0; s < 40; s++) {
    if (s == 20) {
      /* as the game: detached and out of the world, then the group
      ** emptied; the body out of the world */
      H->dJointAttach (j, NONE, NONE);
      H->dWorldRemoveJoint (a, j);
      H->dJointGroupEmpty (g);
      H->dWorldRemoveBody (a, b[2]);
    }
    H->dWorldQuickStep (a, STEP);
    for (int i = 0; i < 4; i++) rec_body (b[i]);
    rec_int (T_FLAG, (uint32_t) H->dBodyIsEnabled (b[1]));
  }
  for (int s = 0; s < 20; s++) {
    H->dxStepBody (b[3], STEP);
    rec_body (b[3]);
  }
  H->dJointGroupDestroy (g);
  H->dBodyDestroy (b[3]);
  H->dBodyDestroy (b[2]);
  H->dWorldDestroy (a);
  H->dWorldDestroy (c);
}

typedef struct Scenario {
  const char *name;
  void (*run) (void);
  int nodaz;
  uint32_t seed;
} Scenario;

static const Scenario SCENARIOS[] = {
  {"library", s_library, 0, 1},
  {"x87", s_x87, 0, 2},
  {"normalise", s_normalise, 0, 3},
  {"free", s_free, 0, 4},
  {"joints", s_joints, 0, 5},
  {"contacts", s_contacts, 0, 6},
  {"contacts-exact", s_contacts_exact, 0, 7},
  {"heavy", s_heavy, 0, 8},
  {"pairs", s_pairs, 0, 9},
  {"rays", s_rays, 0, 10},
  {"worlds", s_worlds, 0, 11},
  /* the same inputs as joints and contacts */
  {"joints-nodaz", s_joints, 1, 5},
  {"contacts-nodaz", s_contacts, 1, 6},
};

int volya_ode_scenario_count (void)
{
  return (int) (sizeof SCENARIOS / sizeof SCENARIOS[0]);
}

const char *volya_ode_scenario_name (int i)
{
  return SCENARIOS[i].name;
}

void volya_ode_scenario_run (const OdeHarness *h, int i)
{
  H = h;
  seed = SCENARIOS[i].seed;
  /* 24-bit precision, round to nearest, exceptions masked; MXCSR masked,
  ** with flush-to-zero and denormals-are-zero unless nodaz */
  h->fpenv (0x007F, SCENARIOS[i].nodaz ? 0x1F80 : 0x9FC0);
  h->srand (1);
  SCENARIOS[i].run ();
}
