/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The physics library comparison (docs/testing.md, The physics library
** against the shipped one): scenarios.c drives a library through ODE 0.5.0's
** public API (ODE's published names and types, declared here without its
** headers) and writes what it computes. win32.c runs them against the game's
** bin\ode.dll, crates/volya-engine-core's build against ours.
**
** Output: per scenario, records of little-endian 32-bit words: the record's
** tag, its word count n, then n words (floats as their bits).
*/

#ifndef VOLYA_ODE_HARNESS_H
#define VOLYA_ODE_HARNESS_H

#include <stdint.h>

typedef float dReal;
typedef struct dxWorld *dWorldID;
typedef struct dxBody *dBodyID;
typedef struct dxJoint *dJointID;
typedef struct dxJointGroup *dJointGroupID;
typedef struct dxGeom *dGeomID;
typedef struct dxSpace *dSpaceID;

typedef struct dMass {
  dReal mass;
  dReal c[4];
  dReal I[12];
} dMass;

typedef struct dSurfaceParameters {
  int mode;
  dReal mu, mu2, bounce, bounce_vel, soft_erp, soft_cfm, motion1, motion2, slip1, slip2;
} dSurfaceParameters;

typedef struct dContactGeom {
  dReal pos[4];
  dReal normal[4];
  dReal depth;
  dGeomID g1, g2;
} dContactGeom;

typedef struct dContact {
  dSurfaceParameters surface;
  dContactGeom geom;
  dReal fdir1[4];
} dContact;

typedef struct dJointFeedback {
  dReal f1[4], t1[4], f2[4], t2[4];
} dJointFeedback;

typedef void dNearCallback (void *data, dGeomID o1, dGeomID o2);

enum {
  dContactMu2 = 0x001, dContactBounce = 0x004, dContactSoftERP = 0x008,
  dContactSoftCFM = 0x010, dContactApprox1 = 0x3000
};

enum {
  dParamLoStop = 0, dParamHiStop, dParamVel, dParamFMax, dParamFudgeFactor,
  dParamBounce, dParamCFM, dParamStopERP, dParamStopCFM, dParamSuspensionERP,
  dParamSuspensionCFM, dParamGroup = 0x100
};

/* The library's functions the scenarios call: X(return type, name, params). */
#define VOLYA_ODE_API(X) \
  X(dWorldID, dWorldCreate, (void)) \
  X(void, dWorldDestroy, (dWorldID)) \
  X(void, dWorldSetGravity, (dWorldID, dReal, dReal, dReal)) \
  X(void, dWorldGetGravity, (dWorldID, dReal *)) \
  X(void, dWorldSetERP, (dWorldID, dReal)) \
  X(dReal, dWorldGetERP, (dWorldID)) \
  X(void, dWorldSetCFM, (dWorldID, dReal)) \
  X(dReal, dWorldGetCFM, (dWorldID)) \
  X(void, dWorldSetQuickStepNumIterations, (dWorldID, int)) \
  X(int, dWorldGetQuickStepNumIterations, (dWorldID)) \
  X(dReal, dWorldGetQuickStepW, (dWorldID)) \
  X(dReal, dWorldGetContactSurfaceLayer, (dWorldID)) \
  X(dReal, dWorldGetContactMaxCorrectingVel, (dWorldID)) \
  X(dReal, dWorldGetAutoDisableLinearThreshold, (dWorldID)) \
  X(int, dWorldGetAutoDisableSteps, (dWorldID)) \
  X(int, dWorldGetAutoDisableFlag, (dWorldID)) \
  X(void, dWorldStep, (dWorldID, dReal)) \
  X(void, dWorldQuickStep, (dWorldID, dReal)) \
  X(void, dWorldAddBody, (dWorldID, dBodyID)) \
  X(void, dWorldRemoveBody, (dWorldID, dBodyID)) \
  X(void, dWorldAddJoint, (dWorldID, dJointID)) \
  X(void, dWorldRemoveJoint, (dWorldID, dJointID)) \
  X(dBodyID, dBodyCreate, (dWorldID)) \
  X(void, dBodyDestroy, (dBodyID)) \
  X(void, dBodySetPosition, (dBodyID, dReal, dReal, dReal)) \
  X(void, dBodySetRotation, (dBodyID, const dReal *)) \
  X(void, dBodySetLinearVel, (dBodyID, dReal, dReal, dReal)) \
  X(void, dBodySetAngularVel, (dBodyID, dReal, dReal, dReal)) \
  X(const dReal *, dBodyGetPosition, (dBodyID)) \
  X(const dReal *, dBodyGetQuaternion, (dBodyID)) \
  X(const dReal *, dBodyGetRotation, (dBodyID)) \
  X(const dReal *, dBodyGetLinearVel, (dBodyID)) \
  X(const dReal *, dBodyGetAngularVel, (dBodyID)) \
  X(void, dBodySetMass, (dBodyID, const dMass *)) \
  X(void, dBodyAddForce, (dBodyID, dReal, dReal, dReal)) \
  X(void, dBodyAddTorque, (dBodyID, dReal, dReal, dReal)) \
  X(void, dBodyAddRelForceAtRelPos, (dBodyID, dReal, dReal, dReal, dReal, dReal, dReal)) \
  X(void, dBodySetGravityMode, (dBodyID, int)) \
  X(void, dBodyDisable, (dBodyID)) \
  X(int, dBodyIsEnabled, (dBodyID)) \
  X(void, dBodySetFiniteRotationMode, (dBodyID, int)) \
  X(void, dBodySetFiniteRotationAxis, (dBodyID, dReal, dReal, dReal)) \
  X(void, dMassSetBoxTotal, (dMass *, dReal, dReal, dReal, dReal)) \
  X(void, dMassSetSphereTotal, (dMass *, dReal, dReal)) \
  X(void, dMassSetCappedCylinderTotal, (dMass *, dReal, int, dReal, dReal)) \
  X(void, dRFromAxisAndAngle, (dReal *, dReal, dReal, dReal, dReal)) \
  X(void, dNormalize3_slow, (dReal *)) \
  X(void, dxStepBody, (dBodyID, dReal)) \
  X(dJointGroupID, dJointGroupCreate, (int)) \
  X(void, dJointGroupDestroy, (dJointGroupID)) \
  X(void, dJointGroupEmpty, (dJointGroupID)) \
  X(dJointID, dJointCreateBall, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateHinge, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateSlider, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateUniversal, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateHinge2, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateFixed, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateAMotor, (dWorldID, dJointGroupID)) \
  X(dJointID, dJointCreateContact, (dWorldID, dJointGroupID, const dContact *)) \
  X(dJointID, dJointCreateContactSpecial, (dWorldID, dJointGroupID, const dContact *)) \
  X(void, dJointAttach, (dJointID, dBodyID, dBodyID)) \
  X(void, dJointDestroy, (dJointID)) \
  X(void, dJointSetFeedback, (dJointID, dJointFeedback *)) \
  X(void, dJointSetBallAnchor, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHingeAnchor, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHingeAxis, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHingeParam, (dJointID, int, dReal)) \
  X(dReal, dJointGetHingeAngle, (dJointID)) \
  X(dReal, dJointGetHingeAngleRate, (dJointID)) \
  X(void, dJointSetSliderAxis, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetSliderParam, (dJointID, int, dReal)) \
  X(dReal, dJointGetSliderPosition, (dJointID)) \
  X(void, dJointSetUniversalAnchor, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetUniversalAxis1, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetUniversalAxis2, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHinge2Anchor, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHinge2Axis1, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHinge2Axis2, (dJointID, dReal, dReal, dReal)) \
  X(void, dJointSetHinge2Param, (dJointID, int, dReal)) \
  X(void, dJointSetFixed, (dJointID)) \
  X(void, dJointSetFixedQuaternionPos, (dJointID, const dReal *, const dReal *)) \
  X(void, dJointSetAMotorMode, (dJointID, int)) \
  X(void, dJointSetAMotorNumAxes, (dJointID, int)) \
  X(void, dJointSetAMotorAxis, (dJointID, int, int, dReal, dReal, dReal)) \
  X(void, dJointSetAMotorParam, (dJointID, int, dReal)) \
  X(dSpaceID, dSimpleSpaceCreate, (dSpaceID)) \
  X(void, dSpaceDestroy, (dSpaceID)) \
  X(void, dSpaceCollide, (dSpaceID, void *, dNearCallback *)) \
  X(dGeomID, dCreateSphere, (dSpaceID, dReal)) \
  X(dGeomID, dCreateBox, (dSpaceID, dReal, dReal, dReal)) \
  X(dGeomID, dCreateCCylinder, (dSpaceID, dReal, dReal)) \
  X(dGeomID, dCreatePlane, (dSpaceID, dReal, dReal, dReal, dReal)) \
  X(dGeomID, dCreateRay, (dSpaceID, dReal)) \
  X(void, dGeomSetBody, (dGeomID, dBodyID)) \
  X(dBodyID, dGeomGetBody, (dGeomID)) \
  X(void, dGeomSetPosition, (dGeomID, dReal, dReal, dReal)) \
  X(void, dGeomSetRotation, (dGeomID, const dReal *)) \
  X(const dReal *, dGeomGetPosition, (dGeomID)) \
  X(const dReal *, dGeomGetRotation, (dGeomID)) \
  X(void, dGeomDestroy, (dGeomID)) \
  X(void, dGeomRaySet, (dGeomID, dReal, dReal, dReal, dReal, dReal, dReal)) \
  X(int, dCollide, (dGeomID, dGeomID, int, dContactGeom *, int)) \
  X(int, dCollideRaySphere, (dGeomID, dGeomID, int, dContactGeom *, int)) \
  X(int, dCollideRayBox, (dGeomID, dGeomID, int, dContactGeom *, int)) \
  X(int, dCollideRayCCylinder, (dGeomID, dGeomID, int, dContactGeom *, int)) \
  X(int, dAreConnected, (dBodyID, dBodyID))

#define VOLYA_ODE_FIELD(ret, name, params) ret (*name) params;

/* What a run provides: the library, the environment and the output. */
typedef struct OdeHarness {
  VOLYA_ODE_API(VOLYA_ODE_FIELD)
  /* the x87 control word and MXCSR the shipped library's thread runs
  ** with (docs/behavior/physics-library.md 7.2); ours ignores them */
  void (*fpenv) (uint16_t x87, uint32_t mxcsr);
  /* sets the C run time's rand state of this thread (its srand) */
  void (*srand) (uint32_t seed);
  /* fsqrt (0), fsin (1), fcos (2) of x, or fpatan (3) of y over x, under
  ** the current control word, as a float store gives it */
  float (*x87) (int op, float y, float x);
  /* appends a record to the current scenario's output */
  void (*record) (void *ctx, uint32_t tag, const uint32_t *words, uint32_t n);
  void *ctx;
} OdeHarness;

/* The scenarios: their count, names, and running one. */
int volya_ode_scenario_count (void);
const char *volya_ode_scenario_name (int i);
void volya_ode_scenario_run (const OdeHarness *h, int i);

#endif
